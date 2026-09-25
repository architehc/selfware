//! Performance Metrics Collection
//!
//! Append-only metrics store for tracking agent performance over time.
//! Used by the self-improvement loop to measure effectiveness of changes.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Schema of the snapshots this build writes. Lines without a
/// `schema_version` were written before the terminal-outcome fix and are not
/// loaded (see [`MetricsStore`]).
pub const PERFORMANCE_SNAPSHOT_SCHEMA: u32 = 1;

/// How a task run ended. One snapshot is recorded per terminal outcome, so a
/// failed, timed-out, budget-stopped or interrupted run is in the denominator
/// exactly like a completed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalOutcome {
    /// Natural completion under a non-failure verdict (`REAL_EDIT` or
    /// `NO_CHANGES`; the `failure_mode` tag says which).
    Completed,
    /// Any failure verdict or error that is not one of the stops below.
    Failed,
    /// The wall-clock or per-call time budget stopped the run.
    Timeout,
    /// The token or cost budget stopped the run.
    BudgetStop,
    /// The run was cancelled from outside (Ctrl+C, SIGTERM, supervisor).
    Interrupted,
}

/// Measured facts about one finished task run, gathered from the same
/// counters the terminal result (`SessionResult`) reports.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalRunStats {
    pub outcome: TerminalOutcome,
    /// The classified failure-mode tag (`REAL_EDIT`, `TIMEOUT`, ...); `None`
    /// when the run was stopped before any verdict was classified.
    pub failure_mode: Option<String>,
    /// Agent-loop turns: the counter `SessionResult.num_turns` reports.
    pub loop_turns: usize,
    /// Tool calls logged in the task checkpoint.
    pub tool_calls: usize,
    /// Errors logged in the task checkpoint.
    pub errors_total: usize,
    /// How many of `errors_total` were recovered.
    pub errors_recovered: usize,
    /// Whether the first verification-shaped check of the run passed;
    /// `None` when no verification ran.
    pub first_verification_passed: Option<bool>,
    /// The credited verification verdict on the final tree (the run
    /// summary's `verification:` line); `None` when no check ran.
    pub final_verification_passed: Option<bool>,
    /// Total LLM tokens (input + output) billed to the run: the counter
    /// `SessionResult.usage.total` reports.
    pub llm_total_tokens: u64,
}

/// Performance of one task run, or the average of several.
///
/// Rates that depend on a check are `Option`: a check that did not run is
/// `None` and never counts as a pass (AGENTS.md rule 3). Aggregates average
/// each such rate over the runs where it was measured, and report how often
/// verification did not run at all in `verification_not_run_rate`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSnapshot {
    /// Timestamp of the snapshot
    pub timestamp: u64,
    /// [`PERFORMANCE_SNAPSHOT_SCHEMA`] at write time. Required on load, so
    /// legacy lines (which lack it) are rejected rather than mixed in.
    pub schema_version: u32,
    /// Terminal outcome of the run; `None` on an aggregate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<TerminalOutcome>,
    /// Failure-mode tag of the run; `None` on an aggregate or when no verdict
    /// was classified (see [`TerminalRunStats::failure_mode`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_mode: Option<String>,
    /// Number of task runs this snapshot covers (1 for a single run).
    pub runs: u64,
    /// Fraction of runs whose outcome is [`TerminalOutcome::Completed`].
    pub task_success_rate: f64,
    /// Average agent-loop turns per run (`SessionResult.num_turns`).
    pub avg_loop_turns: f64,
    /// Average tool calls per run (task checkpoint log).
    pub avg_tool_calls: f64,
    /// Recovered / total checkpoint errors (1.0 when there were none).
    pub error_recovery_rate: f64,
    /// Fraction of measured runs whose FIRST verification passed; `None`
    /// when no covered run ran any verification.
    pub first_verification_pass_rate: Option<f64>,
    /// Average total LLM tokens (input + output) per run
    /// (`SessionResult.usage.total`) — not the context-memory estimate.
    pub avg_llm_total_tokens: f64,
    /// Fraction of measured runs whose credited final verification passed;
    /// `None` when no covered run ran any verification.
    pub final_verification_pass_rate: Option<f64>,
    /// Fraction of runs in which no verification ran at all.
    pub verification_not_run_rate: f64,
    /// Average unrecovered checkpoint errors per run (every kind, not only
    /// compilation errors).
    pub unrecovered_errors_per_run: f64,
    /// Optional label (e.g. "pre-improve-42", "post-improve-42")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

fn bool_rate(value: Option<bool>) -> Option<f64> {
    value.map(|v| if v { 1.0 } else { 0.0 })
}

/// Mean of the measured values; `None` when nothing was measured.
fn mean_measured(values: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    let measured: Vec<f64> = values.flatten().collect();
    (!measured.is_empty()).then(|| measured.iter().sum::<f64>() / measured.len() as f64)
}

impl PerformanceSnapshot {
    /// The snapshot of one finished run.
    pub fn from_terminal_run(stats: &TerminalRunStats) -> Self {
        let verification_ran =
            stats.first_verification_passed.is_some() || stats.final_verification_passed.is_some();
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            schema_version: PERFORMANCE_SNAPSHOT_SCHEMA,
            outcome: Some(stats.outcome),
            failure_mode: stats.failure_mode.clone(),
            runs: 1,
            task_success_rate: if stats.outcome == TerminalOutcome::Completed {
                1.0
            } else {
                0.0
            },
            avg_loop_turns: stats.loop_turns as f64,
            avg_tool_calls: stats.tool_calls as f64,
            error_recovery_rate: if stats.errors_total > 0 {
                stats.errors_recovered as f64 / stats.errors_total as f64
            } else {
                1.0
            },
            first_verification_pass_rate: bool_rate(stats.first_verification_passed),
            avg_llm_total_tokens: stats.llm_total_tokens as f64,
            final_verification_pass_rate: bool_rate(stats.final_verification_passed),
            verification_not_run_rate: if verification_ran { 0.0 } else { 1.0 },
            unrecovered_errors_per_run: stats.errors_total.saturating_sub(stats.errors_recovered)
                as f64,
            label: None,
        }
    }

    /// Add a label to this snapshot
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Average several snapshots. Each snapshot counts once; check-dependent
    /// rates average over the snapshots where the check ran.
    pub fn average(snapshots: &[PerformanceSnapshot]) -> Option<PerformanceSnapshot> {
        if snapshots.is_empty() {
            return None;
        }
        let count = snapshots.len() as f64;
        let mean =
            |f: fn(&PerformanceSnapshot) -> f64| snapshots.iter().map(f).sum::<f64>() / count;
        Some(PerformanceSnapshot {
            timestamp: snapshots.last().map(|s| s.timestamp).unwrap_or(0),
            schema_version: PERFORMANCE_SNAPSHOT_SCHEMA,
            outcome: None,
            failure_mode: None,
            runs: snapshots.iter().map(|s| s.runs).sum(),
            task_success_rate: mean(|s| s.task_success_rate),
            avg_loop_turns: mean(|s| s.avg_loop_turns),
            avg_tool_calls: mean(|s| s.avg_tool_calls),
            error_recovery_rate: mean(|s| s.error_recovery_rate),
            first_verification_pass_rate: mean_measured(
                snapshots.iter().map(|s| s.first_verification_pass_rate),
            ),
            avg_llm_total_tokens: mean(|s| s.avg_llm_total_tokens),
            final_verification_pass_rate: mean_measured(
                snapshots.iter().map(|s| s.final_verification_pass_rate),
            ),
            verification_not_run_rate: mean(|s| s.verification_not_run_rate),
            unrecovered_errors_per_run: mean(|s| s.unrecovered_errors_per_run),
            label: Some(format!("avg_of_{}", snapshots.len())),
        })
    }

    /// Compute weighted delta between two snapshots (positive = improvement).
    /// A verification rate missing on either side contributes nothing: an
    /// unmeasured check is neither an improvement nor a regression.
    pub fn effectiveness_delta(&self, before: &PerformanceSnapshot) -> f64 {
        let delta_success = self.task_success_rate - before.task_success_rate;
        let delta_verification = match (
            self.first_verification_pass_rate,
            before.first_verification_pass_rate,
        ) {
            (Some(after), Some(before)) => after - before,
            _ => 0.0,
        };
        let delta_iterations = before.avg_loop_turns - self.avg_loop_turns; // lower is better
        let delta_recovery = self.error_recovery_rate - before.error_recovery_rate;
        let delta_tokens = before.avg_llm_total_tokens - self.avg_llm_total_tokens; // lower is better

        // Normalize token delta to 0-1 scale (cap at 50% improvement)
        let norm_tokens = if before.avg_llm_total_tokens > 0.0 {
            (delta_tokens / before.avg_llm_total_tokens).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let norm_iterations = if before.avg_loop_turns > 0.0 {
            (delta_iterations / before.avg_loop_turns).clamp(-1.0, 1.0)
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

/// Append-only JSONL store for performance snapshots.
///
/// Legacy lines (no `schema_version`) are skipped on load. They were written
/// only by successful completions — failed, timed-out and interrupted runs
/// never wrote one — with `test_pass_rate` copied from success and a token
/// field holding the context-memory estimate. Every legacy sample is
/// therefore survivorship-biased and cannot be corrected after the fact;
/// mixing them with terminal-outcome snapshots would re-inflate the success
/// rate. The file is not rewritten, so the old lines remain for inspection.
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
        Ok(PerformanceSnapshot::average(&self.trend(n)?))
    }

    fn load_all(&self) -> Result<Vec<PerformanceSnapshot>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = std::fs::File::open(&self.path)?;
        let reader = std::io::BufReader::new(file);
        let mut snapshots = Vec::new();
        let mut skipped = 0usize;
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<PerformanceSnapshot>(&line) {
                Ok(snapshot) => snapshots.push(snapshot),
                Err(_) => skipped += 1,
            }
        }
        if skipped > 0 {
            tracing::debug!(
                "Skipped {} legacy or unreadable performance snapshot line(s) in {}",
                skipped,
                self.path.display()
            );
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
