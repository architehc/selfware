//! Report generation for long-running tests.

use super::project::{ProjectResult, ProjectStatus};
use std::collections::HashMap;
use std::path::Path;

/// Summary report for a long-running test session.
#[derive(Debug, Clone)]
pub struct LongRunningReport {
    pub results: Vec<ProjectResult>,
    pub total_duration_secs: u64,
    pub rounds: Vec<RoundSummary>,
}

/// Summary for a single round.
#[derive(Debug, Clone)]
pub struct RoundSummary {
    pub round_num: usize,
    pub name: String,
    pub results: Vec<ProjectResult>,
}

impl LongRunningReport {
    /// Create a new empty report.
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
            total_duration_secs: 0,
            rounds: Vec::new(),
        }
    }

    /// Add a result to the report.
    pub fn add_result(&mut self, result: ProjectResult) {
        self.results.push(result);
    }

    /// Add a round summary.
    pub fn add_round(&mut self, round: RoundSummary) {
        self.rounds.push(round);
    }

    /// Set total duration.
    pub fn set_duration(&mut self, secs: u64) {
        self.total_duration_secs = secs;
    }

    /// Count results by status.
    pub fn count_by_status(&self) -> HashMap<ProjectStatus, usize> {
        let mut counts = HashMap::new();
        for result in &self.results {
            *counts.entry(result.status.clone()).or_insert(0) += 1;
        }
        counts
    }

    /// Calculate success rate (GREEN + PARTIAL) / total.
    pub fn success_rate(&self) -> f64 {
        if self.results.is_empty() {
            return 0.0;
        }

        let successful = self
            .results
            .iter()
            .filter(|r| {
                matches!(
                    r.status,
                    ProjectStatus::Green | ProjectStatus::Partial | ProjectStatus::Compiles
                )
            })
            .count();

        successful as f64 / self.results.len() as f64
    }

    /// Calculate green rate.
    pub fn green_rate(&self) -> f64 {
        if self.results.is_empty() {
            return 0.0;
        }

        let green = self
            .results
            .iter()
            .filter(|r| matches!(r.status, ProjectStatus::Green))
            .count();

        green as f64 / self.results.len() as f64
    }

    /// Generate a markdown summary.
    pub fn to_markdown(&self) -> String {
        let counts = self.count_by_status();
        let green = counts.get(&ProjectStatus::Green).copied().unwrap_or(0);
        let partial = counts.get(&ProjectStatus::Partial).copied().unwrap_or(0);
        let compiles = counts.get(&ProjectStatus::Compiles).copied().unwrap_or(0);
        let wrote = counts.get(&ProjectStatus::Wrote).copied().unwrap_or(0);
        let fail = counts.get(&ProjectStatus::Fail).copied().unwrap_or(0);
        let total = self.results.len();

        let hours = self.total_duration_secs / 3600;
        let minutes = (self.total_duration_secs % 3600) / 60;

        let mut md = format!(
            r#"# Long-Running Test Report

## Summary

| Metric | Value |
|--------|-------|
| Total Projects | {total} |
| Duration | {hours}h{minutes:02}m |
| Success Rate | {:.1}% |
| Green Rate | {:.1}% |

## Results by Status

| Status | Count | Percentage |
|--------|-------|------------|
| 🟢 GREEN | {green} | {:.1}% |
| 🟡 PARTIAL | {partial} | {:.1}% |
| 🔵 COMPILES | {compiles} | {:.1}% |
| ⚪ WROTE | {wrote} | {:.1}% |
| 🔴 FAIL | {fail} | {:.1}% |

## All Results

| # | Project | Status | Time(s) | Steps | Lines | Tests |
|---|---------|--------|---------|-------|-------|-------|
"#,
            self.success_rate() * 100.0,
            self.green_rate() * 100.0,
            green as f64 / total as f64 * 100.0,
            partial as f64 / total as f64 * 100.0,
            compiles as f64 / total as f64 * 100.0,
            wrote as f64 / total as f64 * 100.0,
            fail as f64 / total as f64 * 100.0,
        );

        for (i, result) in self.results.iter().enumerate() {
            let test_str = if result.compiles {
                format!("{}p/{}f", result.tests_passed, result.tests_failed)
            } else {
                "-".to_string()
            };

            md.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} |\n",
                i + 1,
                result.name,
                result.status,
                result.duration_secs,
                result.steps,
                result.src_lines,
                test_str
            ));
        }

        // Add round summaries
        if !self.rounds.is_empty() {
            md.push_str("\n## Round Details\n\n");

            for round in &self.rounds {
                md.push_str(&format!(
                    "### Round {}: {}\n\n",
                    round.round_num, round.name
                ));
                md.push_str("| Project | Status | Time(s) | Tests |\n");
                md.push_str("|---------|--------|---------|-------|\n");

                for result in &round.results {
                    let test_str = if result.compiles {
                        format!("{}p/{}f", result.tests_passed, result.tests_failed)
                    } else {
                        "-".to_string()
                    };

                    md.push_str(&format!(
                        "| {} | {} | {} | {} |\n",
                        result.name, result.status, result.duration_secs, test_str
                    ));
                }

                md.push('\n');
            }
        }

        md
    }

    /// Write the report to a directory.
    pub fn write_to_dir(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;

        // Write main report
        let report_path = dir.join("report.md");
        std::fs::write(&report_path, self.to_markdown())?;

        // Write summary JSON
        let summary = serde_json::json!({
            "total_projects": self.results.len(),
            "total_duration_secs": self.total_duration_secs,
            "success_rate": self.success_rate(),
            "green_rate": self.green_rate(),
            "by_status": {
                "green": self.results.iter().filter(|r| matches!(r.status, ProjectStatus::Green)).count(),
                "partial": self.results.iter().filter(|r| matches!(r.status, ProjectStatus::Partial)).count(),
                "compiles": self.results.iter().filter(|r| matches!(r.status, ProjectStatus::Compiles)).count(),
                "wrote": self.results.iter().filter(|r| matches!(r.status, ProjectStatus::Wrote)).count(),
                "fail": self.results.iter().filter(|r| matches!(r.status, ProjectStatus::Fail)).count(),
            },
            "results": self.results.iter().map(|r| {
                serde_json::json!({
                    "name": r.name,
                    "status": r.status.to_string(),
                    "duration_secs": r.duration_secs,
                    "steps": r.steps,
                    "src_lines": r.src_lines,
                    "compiles": r.compiles,
                    "tests_passed": r.tests_passed,
                    "tests_failed": r.tests_failed,
                    "outcome_label": r.outcome_label,
                })
            }).collect::<Vec<_>>(),
        });

        let json_path = dir.join("summary.json");
        std::fs::write(&json_path, serde_json::to_string_pretty(&summary).unwrap())?;

        Ok(())
    }
}

impl Default for LongRunningReport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/bench_harness/long_running/report/report_test.rs"]
mod tests;
