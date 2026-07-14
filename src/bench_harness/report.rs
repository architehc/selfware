//! Aggregated benchmark report with throughput and latency statistics.

use serde::{Deserialize, Serialize};

use super::config::HarnessConfig;
use super::task::StreamResult;

/// Complete benchmark harness report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessReport {
    /// ISO 8601 timestamp of the run.
    pub timestamp: String,
    /// Model identifier.
    pub model: String,
    /// Endpoint used.
    pub endpoint: String,
    /// Concurrency level.
    pub max_concurrent: usize,
    /// Total tasks submitted.
    pub tasks_total: usize,
    /// Tasks that passed evaluation.
    pub tasks_passed: usize,
    /// Tasks that failed (errors or eval failure).
    pub tasks_failed: usize,
    /// Total prompt tokens consumed.
    pub total_prompt_tokens: u64,
    /// Total completion tokens generated.
    pub total_completion_tokens: u64,
    /// Aggregate tokens/second (completion tokens / wall-clock seconds).
    pub tokens_per_sec: f64,
    /// Latency percentiles in milliseconds.
    pub latency_p50_ms: u64,
    pub latency_p95_ms: u64,
    pub latency_p99_ms: u64,
    /// Average latency in milliseconds.
    pub latency_avg_ms: f64,
    /// Min/max latency.
    pub latency_min_ms: u64,
    pub latency_max_ms: u64,
    /// Average evaluation score across passed tasks.
    pub avg_score: f64,
    /// Total wall-clock duration in seconds.
    pub total_duration_secs: f64,
    /// Error rate (0.0–1.0).
    pub error_rate: f64,
    /// Per-task results.
    pub results: Vec<StreamResult>,
}

impl HarnessReport {
    /// Build a report from collected stream results.
    pub fn from_results(
        config: &HarnessConfig,
        mut results: Vec<StreamResult>,
        total_duration_secs: f64,
    ) -> Self {
        let tasks_total = results.len();
        let tasks_passed = results.iter().filter(|r| r.success).count();
        let tasks_failed = tasks_total - tasks_passed;

        let total_prompt_tokens: u64 = results.iter().map(|r| r.prompt_tokens).sum();
        let total_completion_tokens: u64 = results.iter().map(|r| r.completion_tokens).sum();

        let tokens_per_sec = if total_duration_secs > 0.0 {
            total_completion_tokens as f64 / total_duration_secs
        } else {
            0.0
        };

        // Latency stats — over SUCCESSFUL requests only. A failed request's
        // latency (an immediate connection error or a timeout) is not a
        // meaningful sample of server response time and would skew the
        // percentiles and min/max.
        let mut latencies: Vec<u64> = results
            .iter()
            .filter(|r| r.success)
            .map(|r| r.latency_ms)
            .collect();
        latencies.sort_unstable();

        let latency_p50_ms = percentile(&latencies, 50);
        let latency_p95_ms = percentile(&latencies, 95);
        let latency_p99_ms = percentile(&latencies, 99);
        let latency_avg_ms = if latencies.is_empty() {
            0.0
        } else {
            latencies.iter().sum::<u64>() as f64 / latencies.len() as f64
        };
        let latency_min_ms = latencies.first().copied().unwrap_or(0);
        let latency_max_ms = latencies.last().copied().unwrap_or(0);

        // Average score across tasks with eval results
        let scored: Vec<f64> = results
            .iter()
            .filter_map(|r| r.eval.as_ref().map(|e| e.score))
            .collect();
        let avg_score = if scored.is_empty() {
            0.0
        } else {
            scored.iter().sum::<f64>() / scored.len() as f64
        };

        let error_rate = if tasks_total > 0 {
            tasks_failed as f64 / tasks_total as f64
        } else {
            0.0
        };

        // Sort results by task_id for deterministic output
        results.sort_by(|a, b| a.task_id.cmp(&b.task_id));

        let timestamp = chrono::Utc::now().to_rfc3339();

        Self {
            timestamp,
            model: config.model.clone(),
            endpoint: config.endpoint.clone(),
            max_concurrent: config.max_concurrent,
            tasks_total,
            tasks_passed,
            tasks_failed,
            total_prompt_tokens,
            total_completion_tokens,
            tokens_per_sec,
            latency_p50_ms,
            latency_p95_ms,
            latency_p99_ms,
            latency_avg_ms,
            latency_min_ms,
            latency_max_ms,
            avg_score,
            total_duration_secs,
            error_rate,
            results,
        }
    }

    /// Serialize to JSON string.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Render as a Markdown summary.
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str(&format!("# Benchmark Report — {}\n", self.model));
        md.push_str(&format!(
            "**Date**: {} | **Endpoint**: {} | **Concurrency**: {}\n\n",
            self.timestamp, self.endpoint, self.max_concurrent,
        ));

        md.push_str("## Summary\n\n");
        md.push_str("| Metric | Value |\n|--------|-------|\n");
        md.push_str(&format!(
            "| Tasks | {}/{} passed ({:.0}% error rate) |\n",
            self.tasks_passed,
            self.tasks_total,
            self.error_rate * 100.0,
        ));
        md.push_str(&format!("| Avg Score | {:.1}% |\n", self.avg_score * 100.0,));
        md.push_str(&format!(
            "| Throughput | {:.0} tok/s |\n",
            self.tokens_per_sec,
        ));
        md.push_str(&format!(
            "| Tokens | {} prompt + {} completion |\n",
            format_tokens(self.total_prompt_tokens),
            format_tokens(self.total_completion_tokens),
        ));
        md.push_str(&format!(
            "| Duration | {:.1}s |\n",
            self.total_duration_secs,
        ));

        md.push_str("\n## Latency\n\n");
        md.push_str("| Percentile | ms |\n|-----------|----|\n");
        md.push_str(&format!("| p50 | {} |\n", self.latency_p50_ms));
        md.push_str(&format!("| p95 | {} |\n", self.latency_p95_ms));
        md.push_str(&format!("| p99 | {} |\n", self.latency_p99_ms));
        md.push_str(&format!("| avg | {:.0} |\n", self.latency_avg_ms));
        md.push_str(&format!(
            "| min/max | {}/{} |\n",
            self.latency_min_ms, self.latency_max_ms,
        ));

        md.push_str("\n## Per-Task Results\n\n");
        md.push_str("| Task | Status | Score | Latency | Tokens |\n");
        md.push_str("|------|--------|-------|---------|--------|\n");
        for r in &self.results {
            let status = if r.success { "PASS" } else { "FAIL" };
            let score = r
                .eval
                .as_ref()
                .map(|e| format!("{:.0}%", e.score * 100.0))
                .unwrap_or_else(|| "N/A".into());
            md.push_str(&format!(
                "| {} | {} | {} | {}ms | {}+{} |\n",
                r.task_id, status, score, r.latency_ms, r.prompt_tokens, r.completion_tokens,
            ));
        }

        md
    }

    /// Write report files to the output directory.
    pub fn write_to_dir(&self, dir: &std::path::Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("bench_report.json"), self.to_json()?)?;
        std::fs::write(dir.join("bench_report.md"), self.to_markdown())?;
        Ok(())
    }
}

/// Calculate the nth percentile from a sorted slice.
fn percentile(sorted: &[u64], pct: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = (pct as f64 / 100.0 * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Format a token count with thousand separators.
fn format_tokens(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench_harness::task::EvalResult;

    fn make_result(task_id: &str, success: bool, latency_ms: u64, tokens: u64) -> StreamResult {
        StreamResult {
            task_id: task_id.into(),
            stream_id: 0,
            success,
            response: "test".into(),
            prompt_tokens: tokens,
            completion_tokens: tokens * 2,
            latency_ms,
            eval: Some(EvalResult {
                score: if success { 1.0 } else { 0.0 },
                passed: success,
                details: vec![],
            }),
            error: if success { None } else { Some("error".into()) },
        }
    }

    #[test]
    fn test_report_from_results() {
        let config = HarnessConfig::default();
        let results = vec![
            make_result("t1", true, 100, 50),
            make_result("t2", true, 200, 60),
            make_result("t3", false, 300, 70),
        ];
        let report = HarnessReport::from_results(&config, results, 10.0);
        assert_eq!(report.tasks_total, 3);
        assert_eq!(report.tasks_passed, 2);
        assert_eq!(report.tasks_failed, 1);
        assert_eq!(report.total_prompt_tokens, 180);
        assert_eq!(report.total_completion_tokens, 360);
        assert!((report.tokens_per_sec - 36.0).abs() < 0.1);
        assert_eq!(report.latency_p50_ms, 200);
    }

    #[test]
    fn test_report_markdown() {
        let config = HarnessConfig::default();
        let results = vec![make_result("t1", true, 100, 50)];
        let report = HarnessReport::from_results(&config, results, 5.0);
        let md = report.to_markdown();
        assert!(md.contains("Benchmark Report"));
        assert!(md.contains("t1"));
        assert!(md.contains("PASS"));
    }

    #[test]
    fn percentiles_exclude_failed_requests() {
        let config = HarnessConfig::default();
        // Two fast successes and one FAILED request with a huge latency.
        let results = vec![
            make_result("ok1", true, 100, 10),
            make_result("ok2", true, 120, 10),
            make_result("boom", false, 100_000, 0),
        ];
        let report = HarnessReport::from_results(&config, results, 1.0);
        // The 100_000ms failure must NOT pollute the latency stats.
        assert_eq!(report.latency_min_ms, 100);
        assert!(
            report.latency_max_ms <= 120,
            "max must be a success latency, got {}",
            report.latency_max_ms
        );
        assert!(
            report.latency_p99_ms <= 120,
            "p99 must exclude the failed request, got {}",
            report.latency_p99_ms
        );
    }

    #[test]
    fn test_percentile() {
        assert_eq!(percentile(&[10, 20, 30, 40, 50], 50), 30);
        assert_eq!(percentile(&[10, 20, 30, 40, 50], 95), 50);
        assert_eq!(percentile(&[], 50), 0);
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(1234), "1,234");
        assert_eq!(format_tokens(1234567), "1,234,567");
        assert_eq!(format_tokens(42), "42");
    }
}
