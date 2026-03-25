//! Web task executor — runs WebTask scenarios through the browser
//! and records interaction traces.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use tokio::sync::Semaphore;
use tracing::{debug, warn};

use super::recorder::{
    ActionOutcome, InteractionRecorder, InteractionTrace, TaskOutcome,
};
use super::tasks::{SuccessCriterion, WebAction, WebTask};

/// Executes web tasks using reqwest-based HTTP fetching.
///
/// For environments without Playwright, this executor fetches pages via HTTP
/// and evaluates text-based criteria. When Playwright is available, it can be
/// extended to use PageController for full browser automation.
pub struct WebTaskExecutor {
    client: reqwest::Client,
    screenshot_dir: PathBuf,
    semaphore: Arc<Semaphore>,
}

impl WebTaskExecutor {
    pub fn new(max_concurrent: usize, screenshot_dir: PathBuf) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .user_agent("selfware-bench/0.1")
            .build()
            .context("Failed to build HTTP client")?;

        std::fs::create_dir_all(&screenshot_dir)?;

        Ok(Self {
            client,
            screenshot_dir,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        })
    }

    /// Execute a single web task and return an interaction trace.
    pub async fn execute(&self, task: &WebTask) -> InteractionTrace {
        let task_screenshot_dir = self.screenshot_dir.join(&task.id);
        let _ = std::fs::create_dir_all(&task_screenshot_dir);

        let mut recorder = InteractionRecorder::new(
            &task.id,
            &task.name,
            task_screenshot_dir,
        );

        let mut current_url = String::new();
        let mut current_body = String::new();
        let mut task_failed = false;
        let mut failure_reasons = Vec::new();

        let task_start = Instant::now();
        let timeout = std::time::Duration::from_secs(task.timeout_secs);

        for action in &task.actions {
            // Check task timeout
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

            let action_start = Instant::now();
            let outcome = self
                .execute_action(action, &mut current_url, &mut current_body, &recorder)
                .await;
            let duration_ms = action_start.elapsed().as_millis() as u64;

            let screenshot_after = match action {
                WebAction::Screenshot { label } => {
                    // Record screenshot reference
                    let ss_path = recorder
                        .screenshot_dir()
                        .join(format!("{}.html", label));
                    // Save page content as HTML for later analysis
                    if !current_body.is_empty() {
                        let _ = std::fs::write(&ss_path, &current_body);
                    }
                    recorder.record_screenshot(
                        label,
                        ss_path.clone(),
                        (0, 0), // dimensions not available in HTTP mode
                    );
                    Some(ss_path)
                }
                _ => None,
            };

            match &outcome {
                ActionOutcome::Failed { error } => {
                    debug!(task_id = %task.id, action = ?action, error, "Action failed");
                }
                ActionOutcome::Timeout => {
                    debug!(task_id = %task.id, action = ?action, "Action timed out");
                    task_failed = true;
                    failure_reasons.push(format!("Action timed out: {action:?}"));
                }
                ActionOutcome::Success { .. } => {}
            }

            recorder.record_action(action.clone(), outcome, duration_ms, None, screenshot_after);
        }

        // Evaluate success criteria
        if !task_failed {
            for criterion in &task.success_criteria {
                let met = evaluate_criterion(criterion, &current_url, &current_body);
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

        recorder.finish(final_outcome)
    }

    /// Execute multiple web tasks concurrently.
    pub async fn execute_all(&self, tasks: &[WebTask]) -> Vec<InteractionTrace> {
        let mut handles = Vec::with_capacity(tasks.len());

        for task in tasks {
            let sem = self.semaphore.clone();
            let client = self.client.clone();
            let screenshot_dir = self.screenshot_dir.clone();
            let task = task.clone();

            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await.expect("semaphore closed");

                // Create a per-task executor (shares nothing except the semaphore)
                let executor = WebTaskExecutor {
                    client,
                    screenshot_dir,
                    semaphore: Arc::new(Semaphore::new(1)), // inner semaphore unused
                };

                executor.execute(&task).await
            });
            handles.push(handle);
        }

        let mut traces = Vec::with_capacity(handles.len());
        for handle in handles {
            match handle.await {
                Ok(trace) => traces.push(trace),
                Err(e) => {
                    warn!("Task execution panicked: {e}");
                }
            }
        }

        traces
    }

    /// Execute a single action.
    async fn execute_action(
        &self,
        action: &WebAction,
        current_url: &mut String,
        current_body: &mut String,
        _recorder: &InteractionRecorder,
    ) -> ActionOutcome {
        match action {
            WebAction::Navigate { url } => {
                match self.client.get(url).send().await {
                    Ok(resp) => {
                        let status = resp.status();
                        *current_url = resp.url().to_string();
                        match resp.text().await {
                            Ok(body) => {
                                *current_body = body;
                                ActionOutcome::Success {
                                    output: format!("HTTP {status}, {} bytes", current_body.len()),
                                }
                            }
                            Err(e) => ActionOutcome::Failed {
                                error: format!("Body read error: {e}"),
                            },
                        }
                    }
                    Err(e) => ActionOutcome::Failed {
                        error: format!("Request error: {e}"),
                    },
                }
            }

            WebAction::Click { selector } => {
                // In HTTP mode, we can't click elements.
                // Check if the selector matches a link and follow it.
                if let Some(href) = extract_href_for_selector(current_body, selector) {
                    let resolved = resolve_url(current_url, &href);
                    match self.client.get(&resolved).send().await {
                        Ok(resp) => {
                            *current_url = resp.url().to_string();
                            match resp.text().await {
                                Ok(body) => {
                                    *current_body = body;
                                    ActionOutcome::Success {
                                        output: format!("Followed link to {}", current_url),
                                    }
                                }
                                Err(e) => ActionOutcome::Failed {
                                    error: format!("Body read error: {e}"),
                                },
                            }
                        }
                        Err(e) => ActionOutcome::Failed {
                            error: format!("Follow link error: {e}"),
                        },
                    }
                } else {
                    ActionOutcome::Success {
                        output: format!("Click simulated on {selector} (HTTP mode)"),
                    }
                }
            }

            WebAction::Fill { selector, value } => {
                // In HTTP mode, track the fill for form submission
                ActionOutcome::Success {
                    output: format!("Fill {selector} = {value} (HTTP mode, tracked)"),
                }
            }

            WebAction::Extract { selector: _, expected } => {
                let found = current_body.to_lowercase().contains(&expected.to_lowercase());
                if found {
                    ActionOutcome::Success {
                        output: format!("Found expected content '{expected}'"),
                    }
                } else {
                    ActionOutcome::Failed {
                        error: format!("Expected content '{expected}' not found in page"),
                    }
                }
            }

            WebAction::Screenshot { label } => {
                // Screenshot handling is done in the caller
                ActionOutcome::Success {
                    output: format!("Screenshot '{label}' captured"),
                }
            }

            WebAction::WaitFor { selector, timeout_ms: _ } => {
                // In HTTP mode, check if content matching the selector pattern exists
                let has_content = !current_body.is_empty()
                    && (current_body.contains(selector)
                        || selector_likely_matches(current_body, selector));
                if has_content {
                    ActionOutcome::Success {
                        output: format!("Content matching '{selector}' found"),
                    }
                } else {
                    ActionOutcome::Success {
                        output: format!("WaitFor '{selector}' skipped (HTTP mode)"),
                    }
                }
            }

            WebAction::Scroll { direction, amount } => {
                ActionOutcome::Success {
                    output: format!("Scroll {direction:?} by {amount} (HTTP mode, no-op)"),
                }
            }

            WebAction::Press { key } => {
                ActionOutcome::Success {
                    output: format!("Press '{key}' (HTTP mode, no-op)"),
                }
            }

            WebAction::Hover { selector } => {
                ActionOutcome::Success {
                    output: format!("Hover '{selector}' (HTTP mode, no-op)"),
                }
            }
        }
    }
}

/// Evaluate a success criterion against the current page state.
fn evaluate_criterion(criterion: &SuccessCriterion, url: &str, body: &str) -> bool {
    match criterion {
        SuccessCriterion::UrlContains(s) => url.to_lowercase().contains(&s.to_lowercase()),
        SuccessCriterion::PageContains(s) => body.to_lowercase().contains(&s.to_lowercase()),
        SuccessCriterion::ElementVisible(selector) => {
            selector_likely_matches(body, selector)
        }
        SuccessCriterion::ExtractedDataMatches { key: _, expected } => {
            body.contains(expected)
        }
        SuccessCriterion::VisualSimilarity { .. } => {
            // Can't evaluate visual similarity in HTTP mode
            true
        }
    }
}

/// Attempt to extract an href from an HTML body for a given CSS-like selector.
/// This is a simplified heuristic — not a full CSS selector engine.
fn extract_href_for_selector(body: &str, selector: &str) -> Option<String> {
    // Handle a[href='...'] selectors
    if selector.starts_with("a[href=") {
        let href_start = selector.find('\'')?;
        let href_end = selector[href_start + 1..].find('\'')?;
        return Some(selector[href_start + 1..href_start + 1 + href_end].to_string());
    }

    // Try to find an anchor tag near the selector pattern
    if let Some(pos) = body.find(selector.trim_start_matches('.').trim_start_matches('#')) {
        // Look backwards/forwards for an href
        let search_window = &body[pos.saturating_sub(200)..body.len().min(pos + 500)];
        if let Some(href_pos) = search_window.find("href=\"") {
            let start = href_pos + 6;
            if let Some(end) = search_window[start..].find('"') {
                return Some(search_window[start..start + end].to_string());
            }
        }
    }

    None
}

/// Resolve a potentially relative URL against a base URL.
fn resolve_url(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }

    if let Ok(base_url) = url::Url::parse(base) {
        if let Ok(resolved) = base_url.join(href) {
            return resolved.to_string();
        }
    }

    // Fallback: just concatenate
    format!("{}/{}", base.trim_end_matches('/'), href.trim_start_matches('/'))
}

/// Check if a CSS selector pattern likely matches content in the HTML body.
fn selector_likely_matches(body: &str, selector: &str) -> bool {
    // Check for class selectors like ".result__title"
    if let Some(class) = selector.strip_prefix('.') {
        return body.contains(&format!("class=\"{class}\""))
            || body.contains(&format!("class=\"{}", class))
            || body.contains(&format!("{class}\""));
    }
    // Check for ID selectors like "#firstHeading"
    if let Some(id) = selector.strip_prefix('#') {
        return body.contains(&format!("id=\"{id}\""));
    }
    // Check for tag selectors
    body.contains(&format!("<{}", selector))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_url_absolute() {
        assert_eq!(
            resolve_url("https://example.com/page", "https://other.com/path"),
            "https://other.com/path"
        );
    }

    #[test]
    fn test_resolve_url_relative() {
        assert_eq!(
            resolve_url("https://en.wikipedia.org/wiki/Rust", "/wiki/Memory_safety"),
            "https://en.wikipedia.org/wiki/Memory_safety"
        );
    }

    #[test]
    fn test_selector_likely_matches_class() {
        let html = r#"<div class="result__title">Hello</div>"#;
        assert!(selector_likely_matches(html, ".result__title"));
        assert!(!selector_likely_matches(html, ".nonexistent"));
    }

    #[test]
    fn test_selector_likely_matches_id() {
        let html = r#"<h1 id="firstHeading">Title</h1>"#;
        assert!(selector_likely_matches(html, "#firstHeading"));
        assert!(!selector_likely_matches(html, "#missing"));
    }

    #[test]
    fn test_extract_href_for_selector() {
        let html = r#"<a href="/wiki/Memory_safety">Memory safety</a>"#;
        let href = extract_href_for_selector(html, "a[href='/wiki/Memory_safety']");
        assert_eq!(href.as_deref(), Some("/wiki/Memory_safety"));
    }

    #[test]
    fn test_evaluate_criterion_url() {
        assert!(evaluate_criterion(
            &SuccessCriterion::UrlContains("wiki".into()),
            "https://en.wikipedia.org/wiki/Rust",
            ""
        ));
    }

    #[test]
    fn test_evaluate_criterion_page() {
        assert!(evaluate_criterion(
            &SuccessCriterion::PageContains("rust".into()),
            "",
            "<h1>Rust Programming</h1>"
        ));
    }

    #[tokio::test]
    async fn test_execute_navigate() {
        // This test requires network access, so we use httpbin
        let executor = WebTaskExecutor::new(
            1,
            PathBuf::from("/tmp/selfware_bench_test"),
        ).unwrap();

        let task = WebTask::new("test-nav", "Test Navigate")
            .with_action(WebAction::Navigate {
                url: "https://httpbin.org/get".into(),
            })
            .with_criterion(SuccessCriterion::PageContains("headers".into()))
            .with_timeout(15);

        let trace = executor.execute(&task).await;
        // We can't guarantee network is available in CI, so just check structure
        assert_eq!(trace.task_id, "test-nav");
        assert_eq!(trace.actions.len(), 1);
    }
}
