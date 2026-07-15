//! Concurrent benchmark runner with semaphore-bounded parallelism.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::Client;
use serde_json::json;

use tracing::{debug, error, info, warn};

use super::config::HarnessConfig;
use super::report::HarnessReport;
use super::task::{BenchTask, StreamResult};

/// Orchestrates concurrent benchmark execution with bounded parallelism.
pub struct HarnessRunner {
    config: HarnessConfig,
    client: Client,
    sequence: Arc<AtomicU64>,
}

impl HarnessRunner {
    /// Create a new runner from config.
    pub fn new(config: HarnessConfig) -> Result<Self> {
        config
            .validate()
            .map_err(|e| anyhow::anyhow!("Invalid config: {e}"))?;

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .connect_timeout(std::time::Duration::from_secs(15))
            .pool_max_idle_per_host(config.max_concurrent)
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            config,
            client,
            sequence: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Run all tasks concurrently (bounded by max_concurrent) and return a report.
    pub async fn run(&self, tasks: Vec<BenchTask>) -> Result<HarnessReport> {
        let run_start = Instant::now();
        let total_tasks = tasks.len();

        info!(
            "Starting benchmark: {} tasks, {} concurrent streams, model={}",
            total_tasks, self.config.max_concurrent, self.config.model,
        );

        // Use a buffered stream so we only poll `max_concurrent` tasks at a
        // time instead of spawning every task eagerly.
        let mut stream = futures::stream::iter(tasks)
            .map(|task| {
                let client = self.client.clone();
                let config = self.config.clone();
                let seq = self.sequence.clone();
                async move {
                    let stream_id = seq.fetch_add(1, Ordering::Relaxed) as usize;
                    execute_task(&client, &config, &task, stream_id).await
                }
            })
            .buffer_unordered(self.config.max_concurrent.max(1));

        let mut results = Vec::with_capacity(total_tasks);
        let mut completed = 0usize;
        while let Some(result) = stream.next().await {
            completed += 1;
            let status = if result.success { "OK" } else { "FAIL" };
            let score_str = result
                .eval
                .as_ref()
                .map(|e| format!("{:.0}%", e.score * 100.0))
                .unwrap_or_else(|| "N/A".into());
            eprintln!(
                "  [{completed}/{total_tasks}] {} — {status} | {score_str} | {}ms | {}+{} tokens",
                result.task_id, result.latency_ms, result.prompt_tokens, result.completion_tokens,
            );
            results.push(result);
        }

        let total_duration = run_start.elapsed();

        let report =
            HarnessReport::from_results(&self.config, results, total_duration.as_secs_f64());

        info!(
            "Benchmark complete: {:.1}s | {}/{} passed | {:.0} tok/s",
            report.total_duration_secs,
            report.tasks_passed,
            report.tasks_total,
            report.tokens_per_sec,
        );

        Ok(report)
    }
}

/// Execute a single benchmark task against the API.
async fn execute_task(
    client: &Client,
    config: &HarnessConfig,
    task: &BenchTask,
    stream_id: usize,
) -> StreamResult {
    let start = Instant::now();
    let url = format!("{}/chat/completions", config.endpoint.trim_end_matches('/'));

    let mut body = json!({
        "model": config.model,
        "messages": task.messages,
        "max_tokens": config.max_tokens,
        "temperature": config.temperature,
        "stream": false,
    });

    // Merge extra_body fields (e.g., chat_template_kwargs for Qwen thinking mode)
    if let (Some(body_obj), Some(extra_obj)) = (body.as_object_mut(), config.extra_body.as_object())
    {
        for (k, v) in extra_obj {
            body_obj.insert(k.clone(), v.clone());
        }
    }

    debug!(task_id = %task.id, stream_id, "Sending request");

    let max_attempts = (config.max_retries + 1).max(1);
    let mut attempt = 0u32;
    let mut delay_ms = config.retry_delay_ms.max(1);
    // Overall per-task deadline: the total time budget for this task INCLUDING
    // all retries and backoffs. Without it, retries each get a fresh
    // per-request timeout and a single task could hold its concurrency slot for
    // retries × timeout_secs (~20 min). timeout_secs is the whole-task budget.
    let deadline = start + std::time::Duration::from_secs(config.timeout_secs);

    let (status, body_text) = loop {
        // Stop before starting another attempt once the task budget is spent.
        if std::time::Instant::now() >= deadline {
            return StreamResult {
                task_id: task.id.clone(),
                stream_id,
                success: false,
                transport_succeeded: false,
                response: String::new(),
                prompt_tokens: 0,
                completion_tokens: 0,
                latency_ms: start.elapsed().as_millis() as u64,
                eval: None,
                error: Some(format!("Task deadline exceeded ({}s)", config.timeout_secs)),
            };
        }
        attempt += 1;
        let mut request = client.post(&url).json(&body);
        if let Some(key) = config.api_key.as_deref() {
            request = request.bearer_auth(key);
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let send_result = match tokio::time::timeout(remaining, request.send()).await {
            Ok(r) => r,
            Err(_elapsed) => {
                return StreamResult {
                    task_id: task.id.clone(),
                    stream_id,
                    success: false,
                    transport_succeeded: false,
                    response: String::new(),
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    latency_ms: start.elapsed().as_millis() as u64,
                    eval: None,
                    error: Some(format!("Task deadline exceeded ({}s)", config.timeout_secs)),
                };
            }
        };
        match send_result {
            Ok(resp) => {
                let status = resp.status();
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                let body_text = match tokio::time::timeout(remaining, resp.text()).await {
                    Ok(r) => r.unwrap_or_default(),
                    Err(_elapsed) => {
                        return StreamResult {
                            task_id: task.id.clone(),
                            stream_id,
                            success: false,
                            transport_succeeded: false,
                            response: String::new(),
                            prompt_tokens: 0,
                            completion_tokens: 0,
                            latency_ms: start.elapsed().as_millis() as u64,
                            eval: None,
                            error: Some(format!(
                                "Task deadline exceeded ({}s)",
                                config.timeout_secs
                            )),
                        };
                    }
                };
                if status.is_success() {
                    break (status, body_text);
                }
                if is_retryable_status(status) && attempt < max_attempts {
                    warn!(
                        task_id = %task.id,
                        stream_id,
                        attempt,
                        max_attempts,
                        "Retryable HTTP {status}; retrying after {delay_ms}ms"
                    );
                    let backoff = std::time::Duration::from_millis(delay_ms)
                        .min(deadline.saturating_duration_since(std::time::Instant::now()));
                    tokio::time::sleep(backoff).await;
                    delay_ms = (delay_ms * 2).min(10_000);
                    continue;
                }
                break (status, body_text);
            }
            Err(e) => {
                if attempt < max_attempts {
                    warn!(
                        task_id = %task.id,
                        stream_id,
                        attempt,
                        max_attempts,
                        "Request failed: {e}; retrying after {delay_ms}ms"
                    );
                    let backoff = std::time::Duration::from_millis(delay_ms)
                        .min(deadline.saturating_duration_since(std::time::Instant::now()));
                    tokio::time::sleep(backoff).await;
                    delay_ms = (delay_ms * 2).min(10_000);
                    continue;
                }
                error!(task_id = %task.id, "Request failed: {e}");
                return StreamResult {
                    task_id: task.id.clone(),
                    stream_id,
                    success: false,
                    response: String::new(),
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    latency_ms: start.elapsed().as_millis() as u64,
                    eval: None,
                    transport_succeeded: false,
                    error: Some(format!("Request error: {e}")),
                };
            }
        }
    };

    if !status.is_success() {
        return StreamResult {
            task_id: task.id.clone(),
            stream_id,
            success: false,
            response: body_text.clone(),
            prompt_tokens: 0,
            completion_tokens: 0,
            latency_ms: start.elapsed().as_millis() as u64,
            eval: None,
            transport_succeeded: false,
            error: Some(format!("HTTP {status}: {body_text}")),
        };
    }

    // Parse OpenAI-compatible response
    let parsed: serde_json::Value = match serde_json::from_str(&body_text) {
        Ok(v) => v,
        Err(e) => {
            return StreamResult {
                task_id: task.id.clone(),
                stream_id,
                success: false,
                response: body_text,
                prompt_tokens: 0,
                completion_tokens: 0,
                latency_ms: start.elapsed().as_millis() as u64,
                eval: None,
                transport_succeeded: false,
                error: Some(format!("JSON parse error: {e}")),
            };
        }
    };

    // Extract content, falling back to reasoning_content if content is null
    // (Qwen3.5 thinking mode returns content=null when all tokens go to reasoning)
    let content = parsed["choices"][0]["message"]["content"]
        .as_str()
        .or_else(|| parsed["choices"][0]["message"]["reasoning_content"].as_str())
        .or_else(|| parsed["choices"][0]["message"]["reasoning"].as_str())
        .unwrap_or("")
        .to_string();

    // Token accounting. If a successful response carries no usage block, do not
    // silently count it as zero — warn, because it makes throughput (tok/s)
    // undercount this request.
    let usage = &parsed["usage"];
    if usage
        .get("completion_tokens")
        .and_then(|v| v.as_u64())
        .is_none()
    {
        warn!(
            task_id = %task.id,
            stream_id,
            "response has no usage.completion_tokens; throughput will undercount this request"
        );
    }
    let prompt_tokens = usage["prompt_tokens"].as_u64().unwrap_or(0);
    let completion_tokens = usage["completion_tokens"].as_u64().unwrap_or(0);

    let latency_ms = start.elapsed().as_millis() as u64;

    // Run evaluation
    let eval = task.evaluator.evaluate(&content);
    let success = eval.passed;

    debug!(
        task_id = %task.id,
        stream_id,
        score = eval.score,
        latency_ms,
        "Task complete"
    );

    StreamResult {
        task_id: task.id.clone(),
        stream_id,
        success,
        transport_succeeded: true,
        response: content,
        prompt_tokens,
        completion_tokens,
        latency_ms,
        eval: Some(eval),
        error: None,
    }
}

fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runner_creation() {
        let config = HarnessConfig::default();
        let runner = HarnessRunner::new(config).unwrap();
        assert_eq!(runner.config.max_concurrent, 32);
    }

    #[test]
    fn test_runner_invalid_config() {
        let config = HarnessConfig {
            endpoint: String::new(),
            ..Default::default()
        };
        assert!(HarnessRunner::new(config).is_err());
    }
}
