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

            // Record screenshot reference for Screenshot actions.
            // The browser already wrote the PNG; we just record the path.
            let screenshot_after = if let WebAction::Screenshot { label } = action {
                if let Some(ref p) = screenshot_path {
                    // Read the real PNG size from the header instead of hardcoding
                    // (0, 0); fall back to (0, 0) only if the file can't be read.
                    let dims = image::image_dimensions(p).unwrap_or((0, 0));
                    recorder.record_screenshot(label, p.clone(), dims);
                }
                screenshot_path.clone()
            } else {
                None
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
    pub async fn execute_all(&self, tasks: &[WebTask]) -> Vec<InteractionTrace> {
        let mut traces = Vec::with_capacity(tasks.len());
        for task in tasks {
            traces.push(self.execute(task).await);
        }
        traces
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
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_navigate_maps_to_goto() {
        let action = WebAction::Navigate {
            url: "https://example.com".into(),
        };
        let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
        assert_eq!(cmd["action"], "goto");
        assert_eq!(cmd["url"], "https://example.com");
        assert_eq!(cmd["wait_until"], "load");
    }

    #[test]
    fn test_click_maps_to_click() {
        let action = WebAction::Click {
            selector: "#button".into(),
        };
        let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
        assert_eq!(cmd["action"], "click");
        assert_eq!(cmd["selector"], "#button");
    }

    #[test]
    fn test_fill_maps_to_fill_with_text_field() {
        let action = WebAction::Fill {
            selector: "input[name='q']".into(),
            value: "hello world".into(),
        };
        let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
        assert_eq!(cmd["action"], "fill");
        assert_eq!(cmd["selector"], "input[name='q']");
        assert_eq!(cmd["text"], "hello world");
        // The tool field is "text", not "value"
        assert!(cmd.get("value").is_none());
    }

    #[test]
    fn test_extract_maps_to_text() {
        let action = WebAction::Extract {
            selector: ".result".into(),
            expected: "Rust".into(),
        };
        let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
        assert_eq!(cmd["action"], "text");
        assert_eq!(cmd["selector"], ".result");
        // Expected is not part of the command
        assert!(cmd.get("expected").is_none());
    }

    #[test]
    fn test_screenshot_maps_to_screenshot() {
        let action = WebAction::Screenshot {
            label: "result_page".into(),
        };
        let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
        assert_eq!(cmd["action"], "screenshot");
        assert_eq!(cmd["full_page"], true);
        let path = cmd["path"].as_str().unwrap();
        assert!(
            path.ends_with("result_page.png"),
            "path should end with result_page.png, got: {path}"
        );
    }

    #[test]
    fn test_waitfor_maps_to_wait_for_visible() {
        let action = WebAction::WaitFor {
            selector: "#content".into(),
            timeout_ms: 5000,
        };
        let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
        assert_eq!(cmd["action"], "wait_for");
        assert_eq!(cmd["selector"], "#content");
        assert_eq!(cmd["state"], "visible");
        assert_eq!(cmd["timeout_ms"], 5000);
    }

    #[test]
    fn test_scroll_down() {
        let action = WebAction::Scroll {
            direction: ScrollDirection::Down,
            amount: 300,
        };
        let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
        assert_eq!(cmd["action"], "evaluate");
        assert_eq!(
            cmd["expression"].as_str().unwrap(),
            "window.scrollBy(0, 300)"
        );
    }

    #[test]
    fn test_scroll_up() {
        let action = WebAction::Scroll {
            direction: ScrollDirection::Up,
            amount: 300,
        };
        let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
        assert_eq!(cmd["action"], "evaluate");
        assert_eq!(
            cmd["expression"].as_str().unwrap(),
            "window.scrollBy(0, -300)"
        );
    }

    #[test]
    fn test_scroll_left() {
        let action = WebAction::Scroll {
            direction: ScrollDirection::Left,
            amount: 300,
        };
        let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
        assert_eq!(cmd["action"], "evaluate");
        assert_eq!(
            cmd["expression"].as_str().unwrap(),
            "window.scrollBy(-300, 0)"
        );
    }

    #[test]
    fn test_scroll_right() {
        let action = WebAction::Scroll {
            direction: ScrollDirection::Right,
            amount: 300,
        };
        let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
        assert_eq!(cmd["action"], "evaluate");
        assert_eq!(
            cmd["expression"].as_str().unwrap(),
            "window.scrollBy(300, 0)"
        );
    }

    #[test]
    fn test_press_maps_to_press() {
        let action = WebAction::Press {
            key: "Enter".into(),
        };
        let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
        assert_eq!(cmd["action"], "press");
        assert_eq!(cmd["key"], "Enter");
    }

    #[test]
    fn test_hover_maps_to_hover() {
        let action = WebAction::Hover {
            selector: ".menu-item".into(),
        };
        let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
        assert_eq!(cmd["action"], "hover");
        assert_eq!(cmd["selector"], ".menu-item");
    }

    #[test]
    fn result_is_visible_accepts_bool_and_object() {
        assert!(result_is_visible(&json!(true)));
        assert!(!result_is_visible(&json!(false)));
        // the bridge returns an object
        assert!(result_is_visible(&json!({"visible": true})));
        assert!(!result_is_visible(&json!({"visible": false})));
        // missing / wrong shape -> not visible
        assert!(!result_is_visible(&json!({})));
        assert!(!result_is_visible(&json!("nope")));
    }

    #[tokio::test]
    async fn execute_all_empty_is_empty() {
        let dir = std::env::temp_dir().join(format!("bx_empty_{}", std::process::id()));
        let ex = BrowserTaskExecutor::new(dir.clone()).unwrap();
        let traces = ex.execute_all(&[]).await;
        assert!(traces.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
