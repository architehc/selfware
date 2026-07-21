//! Browser-backed task executor — runs WebTask scenarios through a real
//! Playwright browser via the production `page_control` tool.
//!
//! Unlike the HTTP-fetch executor in `executor.rs`, this module drives a
//! real browser so interaction outcomes are honest: a click/fill/wait that
//! the browser could not perform is recorded as `Failed`, never "simulated".

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tracing::debug;

use crate::tools::page_controller::PageControlTool;
use crate::tools::Tool;

use super::recorder::{ActionOutcome, InteractionRecorder, InteractionTrace, TaskOutcome};
use super::tasks::{ScrollDirection, SuccessCriterion, WebAction, WebTask};

/// Translate a [`WebAction`] into the JSON command understood by the
/// production `page_control` tool (`PageControlTool::execute`).
///
/// Pure and deterministic so it can be unit-tested without a browser.
pub fn web_action_to_command(action: &WebAction, screenshot_dir: &Path) -> Value {
    match action {
        WebAction::Navigate { url } => json!({
            "action": "goto",
            "url": url,
            "wait_until": "load"
        }),
        WebAction::Click { selector } => json!({
            "action": "click",
            "selector": selector
        }),
        WebAction::Fill { selector, value } => json!({
            "action": "fill",
            "selector": selector,
            "text": value
        }),
        WebAction::Extract { selector, .. } => json!({
            "action": "text",
            "selector": selector
        }),
        WebAction::Screenshot { label } => {
            let path = screenshot_dir.join(format!("{label}.png"));
            json!({
                "action": "screenshot",
                "path": path.to_string_lossy(),
                "full_page": true
            })
        }
        WebAction::WaitFor {
            selector,
            timeout_ms,
        } => json!({
            "action": "wait_for",
            "selector": selector,
            "state": "visible",
            "timeout_ms": timeout_ms
        }),
        WebAction::Scroll { direction, amount } => {
            let (dx, dy) = match direction {
                ScrollDirection::Up => (0, -amount),
                ScrollDirection::Down => (0, *amount),
                ScrollDirection::Left => (-*amount, 0),
                ScrollDirection::Right => (*amount, 0),
            };
            json!({
                "action": "evaluate",
                "expression": format!("window.scrollBy({dx}, {dy})")
            })
        }
        WebAction::Press { key } => json!({
            "action": "press",
            "key": key
        }),
        WebAction::Hover { selector } => json!({
            "action": "hover",
            "selector": selector
        }),
    }
}

/// Stringify a JSON value for human-readable output.
fn result_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        // The bridge wraps extracted content in an object: {"text": "..."} for a
        // single element, {"texts": [...]} for many, {"url": "..."} for the url
        // action. Use the inner text rather than a JSON dump of the wrapper
        // (which would include the field names and mis-match `contains` checks).
        Value::Object(_) => {
            if let Some(s) = v.get("text").and_then(|t| t.as_str()) {
                s.to_string()
            } else if let Some(s) = v.get("url").and_then(|t| t.as_str()) {
                s.to_string()
            } else if let Some(arr) = v.get("texts").and_then(|t| t.as_array()) {
                arr.iter()
                    .filter_map(|e| e.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                serde_json::to_string(v).unwrap_or_default()
            }
        }
        _ => serde_json::to_string(v).unwrap_or_default(),
    }
}

/// Interpret the `result` payload of a `visible` bridge action. The bridge
/// returns either a bare boolean or an object like `{ "visible": true }`, so
/// accept both rather than only a bare bool (which silently reads as false).
fn result_is_visible(result: &Value) -> bool {
    result
        .as_bool()
        .or_else(|| result.get("visible").and_then(|b| b.as_bool()))
        .unwrap_or(false)
}

/// Truncate a string to approximately `max_len` characters, appending "...".
fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() > max_len {
        let truncated: String = s.chars().take(max_len).collect();
        format!("{truncated}...")
    } else {
        s.to_string()
    }
}

/// Executes [`WebTask`]s through a real Playwright browser via the production
/// `page_control` tool.
///
/// Unlike the HTTP executor, outcomes are honest: a click/fill/wait that the
/// browser could not perform is recorded as `Failed`, never "simulated".
pub struct BrowserTaskExecutor {
    screenshot_dir: PathBuf,
}

impl BrowserTaskExecutor {
    /// Create a new browser executor.
    ///
    /// The `screenshot_dir` is the root directory where per-task screenshot
    /// subdirectories will be created.
    pub fn new(screenshot_dir: PathBuf) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&screenshot_dir)?;
        Ok(Self { screenshot_dir })
    }

    /// Execute one task in a fresh browser session and return its trace.
    pub async fn execute(&self, task: &WebTask) -> InteractionTrace {
        let task_screenshot_dir = self.screenshot_dir.join(&task.id);
        let _ = std::fs::create_dir_all(&task_screenshot_dir);

        let mut recorder = InteractionRecorder::new(&task.id, &task.name, task_screenshot_dir);

        let tool = PageControlTool::new();

        let mut task_failed = false;
        let mut failure_reasons = Vec::new();

        let task_start = Instant::now();
        let timeout = Duration::from_secs(task.timeout_secs);

        for action in &task.actions {
            // Check task-level timeout
            if task_start.elapsed() > timeout {
                recorder.record_action(
                    action.clone(),
                    ActionOutcome::Timeout,
                    task_start.elapsed().as_millis() as u64,
                    None,
                    None,
                );
                task_failed = true;
                failure_reasons.push("Task timeout exceeded".into());
                break;
            }

            let command = web_action_to_command(action, recorder.screenshot_dir());
            let screenshot_path: Option<PathBuf> = command
                .get("path")
                .and_then(|p| p.as_str())
                .map(PathBuf::from);

            // Bound this action by the time remaining in the task budget, so a
            // single slow action cannot overrun the task timeout that is only
            // checked between actions.
            let remaining = timeout.saturating_sub(task_start.elapsed());
            let action_start = Instant::now();
            let outcome = match tokio::time::timeout(remaining, tool.execute(command)).await {
                Err(_elapsed) => ActionOutcome::Timeout,
                Ok(Err(e)) => ActionOutcome::Failed {
                    error: e.to_string(),
                },
                Ok(Ok(v)) => {
                    let success = v.get("success").and_then(|s| s.as_bool());
                    match success {
                        Some(true) => {
                            // For Extract, additionally check the returned text
                            // against `expected` (case-insensitive).
                            if let WebAction::Extract { expected, .. } = action {
                                let result_text = result_to_string(&v["result"]);
                                if result_text
                                    .to_lowercase()
                                    .contains(&expected.to_lowercase())
                                {
                                    ActionOutcome::Success {
                                        output: truncate(&result_text, 200),
                                    }
                                } else {
                                    ActionOutcome::Failed {
                                        error: format!("expected '{expected}' not found"),
                                    }
                                }
                            } else {
                                ActionOutcome::Success {
                                    output: truncate(&result_to_string(&v["result"]), 200),
                                }
                            }
                        }
                        Some(false) | None => ActionOutcome::Failed {
                            error: v
                                .get("error")
                                .and_then(|e| e.as_str())
                                .unwrap_or("browser action failed")
                                .to_string(),
                        },
                    }
                }
            };

            // Record a screenshot reference only for a SUCCESSFUL Screenshot action.
            // A failed screenshot wrote no valid PNG, so recording its path/size
            // would be misleading.
            let screenshot_after = match (action, &outcome) {
                (WebAction::Screenshot { label }, ActionOutcome::Success { .. }) => {
                    if let Some(ref p) = screenshot_path {
                        // Real PNG size from the header; fall back to (0, 0) only
                        // if the file can't be read.
                        let dims = image::image_dimensions(p).unwrap_or((0, 0));
                        recorder.record_screenshot(label, p.clone(), dims);
                    }
                    screenshot_path.clone()
                }
                _ => None,
            };

            // Handle outcome side effects. A timeout or ANY failed action fails
            // the task and stops it (subsequent actions typically depend on the
            // one that just failed).
            let should_break = match &outcome {
                ActionOutcome::Timeout => {
                    task_failed = true;
                    failure_reasons.push("Task timeout exceeded".into());
                    true
                }
                ActionOutcome::Failed { error } => {
                    // Any failed browser action fails the task: for a benchmark,
                    // an action the browser could not perform means the
                    // automation did not work, and later actions usually depend
                    // on it. Stop here.
                    debug!(
                        task_id = %task.id,
                        action = ?action,
                        error,
                        "Browser action failed"
                    );
                    task_failed = true;
                    failure_reasons.push(format!("{action:?} failed: {error}"));
                    true
                }
                ActionOutcome::Success { .. } => false,
            };

            let duration_ms = action_start.elapsed().as_millis() as u64;
            recorder.record_action(action.clone(), outcome, duration_ms, None, screenshot_after);

            if should_break {
                break;
            }
        }

        // Evaluate success criteria via the browser
        if !task_failed {
            for criterion in &task.success_criteria {
                if matches!(criterion, SuccessCriterion::VisualSimilarity { .. }) {
                    task_failed = true;
                    failure_reasons
                        .push("VisualSimilarity not supported in browser executor".into());
                    continue;
                }
                // Bound each criterion check by the REMAINING task budget so a
                // short-timeout task can't overrun by 30s per failing criterion.
                let remaining = timeout.saturating_sub(task_start.elapsed());
                if remaining.is_zero() {
                    task_failed = true;
                    failure_reasons.push("Task timeout exceeded during criteria evaluation".into());
                    break;
                }
                let met = match tokio::time::timeout(
                    remaining,
                    evaluate_criterion_browser(criterion, &tool),
                )
                .await
                {
                    Ok(met) => met,
                    Err(_) => {
                        task_failed = true;
                        failure_reasons.push("Criterion evaluation timeout".into());
                        false
                    }
                };
                if !met {
                    task_failed = true;
                    failure_reasons.push(format!("Criterion not met: {criterion:?}"));
                }
            }
        }

        let final_outcome = if task_failed {
            if failure_reasons.iter().any(|r| r.contains("timeout")) {
                TaskOutcome::Timeout
            } else {
                TaskOutcome::Failed {
                    reasons: failure_reasons,
                }
            }
        } else {
            TaskOutcome::Passed
        };

        // Always shut down the browser
        let _ = tool.shutdown().await;

        recorder.finish(final_outcome)
    }

    /// Execute several tasks sequentially, each in its own fresh browser
    /// session. Sequential by design: each task drives a full Playwright
    /// browser, so running many at once would spawn many heavyweight browser
    /// processes for little benchmarking value.
    pub async fn execute_all(
        &self,
        tasks: &[WebTask],
        concurrency: usize,
    ) -> Vec<InteractionTrace> {
        use futures::stream::StreamExt;
        // Run up to `concurrency` browser tasks at once — each `execute` spawns
        // its own browser — while preserving task order via `buffered`. This
        // previously ran strictly sequentially despite the advertised
        // `max_browser_concurrent`, so the benchmark never used the parallelism.
        futures::stream::iter(tasks.iter().map(|task| self.execute(task)))
            .buffered(concurrency.max(1))
            .collect()
            .await
    }
}

/// Evaluate a success criterion by querying the live browser.
async fn evaluate_criterion_browser(criterion: &SuccessCriterion, tool: &PageControlTool) -> bool {
    match criterion {
        SuccessCriterion::UrlContains(s) => match tool.execute(json!({"action": "url"})).await {
            Ok(v) => {
                let text = result_to_string(&v["result"]);
                text.to_lowercase().contains(&s.to_lowercase())
            }
            Err(_) => false,
        },
        SuccessCriterion::PageContains(s) => {
            match tool
                .execute(json!({"action": "text", "selector": "body"}))
                .await
            {
                Ok(v) => {
                    let text = result_to_string(&v["result"]);
                    text.to_lowercase().contains(&s.to_lowercase())
                }
                Err(_) => false,
            }
        }
        SuccessCriterion::ElementVisible(sel) => {
            match tool
                .execute(json!({"action": "visible", "selector": sel}))
                .await
            {
                Ok(v) => {
                    let success = v.get("success").and_then(|s| s.as_bool()).unwrap_or(false);
                    success && result_is_visible(&v["result"])
                }
                Err(_) => false,
            }
        }
        SuccessCriterion::ExtractedDataMatches { expected, .. } => {
            match tool
                .execute(json!({"action": "text", "selector": "body"}))
                .await
            {
                Ok(v) => {
                    let text = result_to_string(&v["result"]);
                    text.to_lowercase().contains(&expected.to_lowercase())
                }
                Err(_) => false,
            }
        }
        SuccessCriterion::VisualSimilarity { .. } => false,
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/bench_harness/computer_control/browser_executor/browser_executor_test.rs"]
mod tests;
