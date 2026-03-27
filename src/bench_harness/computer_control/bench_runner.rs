//! Browser benchmark runner — orchestrates web tasks with LLM-driven evaluation
//! and feeds interaction traces into the consolidation pipeline.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use super::executor::WebTaskExecutor;
use super::recorder::{InteractionTrace, TaskOutcome};
use super::tasks::{self, WebTask};

use crate::api::types::Message;
use crate::bench_harness::config::HarnessConfig;
use crate::bench_harness::report::HarnessReport;
use crate::bench_harness::runner::HarnessRunner;
use crate::bench_harness::task::{BenchTask, KeywordEvaluator};

/// Configuration for the browser benchmark.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserBenchConfig {
    /// LLM harness configuration.
    pub harness: HarnessConfig,
    /// Maximum concurrent browser sessions.
    pub max_browser_concurrent: usize,
    /// Directory for screenshots and traces.
    pub output_dir: PathBuf,
    /// Whether to run LLM analysis on captured pages.
    pub llm_analysis: bool,
    /// Whether to store traces for consolidation.
    pub store_traces: bool,
}

impl Default for BrowserBenchConfig {
    fn default() -> Self {
        Self {
            harness: HarnessConfig {
                endpoint: "http://localhost:8000/v1".into(),
                model: "qwen3.5-27b".into(),
                max_concurrent: 16,
                max_tokens: 1024,
                temperature: 0.3,
                timeout_secs: 120,
                output_dir: "bench_results/browser".into(),
                extra_body: serde_json::json!({
                    "chat_template_kwargs": {"enable_thinking": false}
                }),
            },
            max_browser_concurrent: 4, // Browser sessions are heavier than LLM calls
            output_dir: "bench_results/browser".into(),
            llm_analysis: true,
            store_traces: true,
        }
    }
}

/// Results from a browser benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserBenchReport {
    /// Timestamp of the run.
    pub timestamp: String,
    /// Total web tasks executed.
    pub tasks_total: usize,
    /// Tasks that passed.
    pub tasks_passed: usize,
    /// Interaction traces from execution.
    pub traces: Vec<InteractionTrace>,
    /// LLM analysis report (if llm_analysis was enabled).
    pub llm_report: Option<HarnessReport>,
    /// Total duration in seconds.
    pub total_duration_secs: f64,
}

impl BrowserBenchReport {
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str("# Browser Benchmark Report\n\n");
        md.push_str(&format!(
            "**Date**: {} | **Duration**: {:.1}s\n\n",
            self.timestamp, self.total_duration_secs
        ));

        md.push_str("## Web Task Results\n\n");
        md.push_str("| Task | Status | Actions | Duration |\n");
        md.push_str("|------|--------|---------|----------|\n");
        for trace in &self.traces {
            let status = match &trace.final_outcome {
                TaskOutcome::Passed => "PASS",
                TaskOutcome::Failed { .. } => "FAIL",
                TaskOutcome::Timeout => "TIMEOUT",
                TaskOutcome::Error { .. } => "ERROR",
            };
            md.push_str(&format!(
                "| {} | {} | {} | {:.1}s |\n",
                trace.task_id,
                status,
                trace.actions.len(),
                trace.total_duration_ms as f64 / 1000.0,
            ));
        }

        if let Some(llm) = &self.llm_report {
            md.push_str("\n## LLM Analysis\n\n");
            md.push_str("| Metric | Value |\n|--------|-------|\n");
            md.push_str(&format!("| Tasks analyzed | {} |\n", llm.tasks_total));
            md.push_str(&format!("| Avg score | {:.0}% |\n", llm.avg_score * 100.0));
            md.push_str(&format!(
                "| Throughput | {:.0} tok/s |\n",
                llm.tokens_per_sec
            ));
        }

        md
    }

    pub fn write_to_dir(&self, dir: &std::path::Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("browser_bench_report.json"), self.to_json()?)?;
        std::fs::write(dir.join("browser_bench_report.md"), self.to_markdown())?;

        // Write individual traces
        let traces_dir = dir.join("traces");
        std::fs::create_dir_all(&traces_dir)?;
        for trace in &self.traces {
            let path = traces_dir.join(format!("{}.json", trace.task_id));
            std::fs::write(&path, serde_json::to_string_pretty(trace)?)?;
        }

        Ok(())
    }
}

/// Runs browser benchmarks: executes web tasks, optionally analyzes with LLM.
pub struct BrowserBenchRunner {
    config: BrowserBenchConfig,
    executor: WebTaskExecutor,
}

impl BrowserBenchRunner {
    pub fn new(config: BrowserBenchConfig) -> Result<Self> {
        let executor = WebTaskExecutor::new(
            config.max_browser_concurrent,
            config.output_dir.join("screenshots"),
        )?;

        Ok(Self { config, executor })
    }

    /// Run all predefined web task scenarios.
    pub async fn run_all(&self) -> Result<BrowserBenchReport> {
        let scenarios = tasks::all_scenarios();
        self.run_tasks(&scenarios).await
    }

    /// Run specific web tasks.
    pub async fn run_tasks(&self, tasks: &[WebTask]) -> Result<BrowserBenchReport> {
        let start = Instant::now();

        info!(
            "Starting browser benchmark: {} tasks, {} browser concurrent, {} LLM concurrent",
            tasks.len(),
            self.config.max_browser_concurrent,
            self.config.harness.max_concurrent,
        );

        // Phase 1: Execute web tasks and capture traces
        eprintln!("\n--- Phase 1: Executing web tasks ---");
        let traces = self.executor.execute_all(tasks).await;

        let tasks_passed = traces
            .iter()
            .filter(|t| matches!(t.final_outcome, TaskOutcome::Passed))
            .count();

        for trace in &traces {
            let status = match &trace.final_outcome {
                TaskOutcome::Passed => "PASS",
                TaskOutcome::Failed { reasons } => &format!("FAIL: {}", reasons.join("; ")),
                TaskOutcome::Timeout => "TIMEOUT",
                TaskOutcome::Error { message } => message,
            };
            eprintln!(
                "  {} — {} | {} actions | {:.1}s",
                trace.task_id,
                status,
                trace.actions.len(),
                trace.total_duration_ms as f64 / 1000.0,
            );
        }

        eprintln!("\nWeb tasks: {}/{} passed", tasks_passed, traces.len(),);

        // Phase 2: LLM analysis of captured page content
        let llm_report = if self.config.llm_analysis {
            eprintln!("\n--- Phase 2: LLM analysis of captured content ---");
            match self.run_llm_analysis(&traces).await {
                Ok(report) => {
                    eprintln!(
                        "LLM analysis: {}/{} passed | {:.0} tok/s | avg score {:.0}%",
                        report.tasks_passed,
                        report.tasks_total,
                        report.tokens_per_sec,
                        report.avg_score * 100.0,
                    );
                    Some(report)
                }
                Err(e) => {
                    warn!("LLM analysis failed: {e}");
                    None
                }
            }
        } else {
            None
        };

        // Phase 3: Store traces for consolidation
        if self.config.store_traces {
            let traces_dir = self.config.output_dir.join("traces");
            std::fs::create_dir_all(&traces_dir)?;
            for trace in &traces {
                let path = traces_dir.join(format!("{}.json", trace.task_id));
                std::fs::write(&path, serde_json::to_string_pretty(trace)?)?;
            }
            eprintln!("\nTraces saved to {}", traces_dir.display(),);
        }

        let report = BrowserBenchReport {
            timestamp: chrono::Utc::now().to_rfc3339(),
            tasks_total: traces.len(),
            tasks_passed,
            traces,
            llm_report,
            total_duration_secs: start.elapsed().as_secs_f64(),
        };

        info!(
            "Browser benchmark complete: {}/{} passed in {:.1}s",
            report.tasks_passed, report.tasks_total, report.total_duration_secs,
        );

        Ok(report)
    }

    /// Run LLM analysis on the captured page content from traces.
    ///
    /// Creates analysis tasks from the captured HTML content and sends them
    /// through the 16-concurrent-stream harness.
    async fn run_llm_analysis(&self, traces: &[InteractionTrace]) -> Result<HarnessReport> {
        let runner = HarnessRunner::new(self.config.harness.clone())?;

        // Build analysis tasks from traces
        let mut bench_tasks = Vec::new();
        for trace in traces {
            // Gather extracted content from successful actions
            let mut page_summaries = Vec::new();
            for action in &trace.actions {
                if let super::recorder::ActionOutcome::Success { output } = &action.outcome {
                    if output.len() > 20 {
                        page_summaries.push(output.clone());
                    }
                }
            }

            // Read any saved HTML content from screenshots
            for ss in &trace.screenshots {
                if ss.path.exists() {
                    if let Ok(content) = std::fs::read_to_string(&ss.path) {
                        // Strip HTML tags and truncate to keep prompts small
                        let text = strip_html_tags(&content);
                        let truncated = if text.len() > 3000 {
                            format!("{}...[truncated]", &text[..3000])
                        } else {
                            text
                        };
                        page_summaries.push(truncated);
                    }
                }
            }

            if page_summaries.is_empty() {
                continue;
            }

            let content_summary = page_summaries.join("\n---\n");
            let prompt = format!(
                "Analyze this web page content captured during a browser benchmark task.\n\
                 Task: {} ({})\n\n\
                 Captured content:\n{}\n\n\
                 Provide a JSON response with:\n\
                 - \"summary\": What the page contains (1-2 sentences)\n\
                 - \"data_extracted\": Key data points found (array of strings)\n\
                 - \"navigation_quality\": Score 0-100 for how well the task navigated\n\
                 - \"content_relevance\": Score 0-100 for relevance to the task description\n\
                 \nRespond with only valid JSON.",
                trace.task_name,
                trace.task_id,
                &content_summary[..content_summary.len().min(6000)],
            );

            bench_tasks.push(BenchTask {
                id: format!("llm-{}", trace.task_id),
                description: format!("Analyze {} content", trace.task_name),
                messages: vec![
                    Message::system("You are a web content analyst. Output only valid JSON."),
                    Message::user(prompt),
                ],
                evaluator: Box::new(KeywordEvaluator::new(vec![
                    "summary".into(),
                    "data_extracted".into(),
                ])),
            });
        }

        if bench_tasks.is_empty() {
            // Return empty report
            return Ok(HarnessReport::from_results(
                &self.config.harness,
                vec![],
                0.0,
            ));
        }

        runner.run(bench_tasks).await
    }
}

/// Strip HTML tags from content, keeping only text.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut last_was_space = false;

    let lower = html.to_lowercase();
    let chars: Vec<char> = html.chars().collect();
    let lower_chars: Vec<char> = lower.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        if !in_tag && chars[i] == '<' {
            in_tag = true;
            // Check for script/style tags
            let remaining: String = lower_chars[i..].iter().take(10).collect();
            if remaining.starts_with("<script") {
                in_script = true;
            } else if remaining.starts_with("<style") {
                in_style = true;
            } else if remaining.starts_with("</script") {
                in_script = false;
            } else if remaining.starts_with("</style") {
                in_style = false;
            }
        } else if in_tag && chars[i] == '>' {
            in_tag = false;
        } else if !in_tag && !in_script && !in_style {
            let ch = chars[i];
            if ch.is_whitespace() {
                if !last_was_space {
                    result.push(' ');
                    last_was_space = true;
                }
            } else {
                result.push(ch);
                last_was_space = false;
            }
        }
        i += 1;
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_bench_config_default() {
        let cfg = BrowserBenchConfig::default();
        assert_eq!(cfg.harness.max_concurrent, 16);
        assert_eq!(cfg.max_browser_concurrent, 4);
        assert!(cfg.llm_analysis);
        assert!(cfg.store_traces);
    }

    #[test]
    fn test_browser_bench_report_markdown() {
        let report = BrowserBenchReport {
            timestamp: "2026-03-25T00:00:00Z".into(),
            tasks_total: 2,
            tasks_passed: 1,
            traces: vec![],
            llm_report: None,
            total_duration_secs: 10.5,
        };
        let md = report.to_markdown();
        assert!(md.contains("Browser Benchmark Report"));
        assert!(md.contains("10.5s"));
    }
}
