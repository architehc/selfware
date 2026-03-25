//! Concurrent benchmark runner with semaphore-bounded parallelism.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::json;
use tokio::sync::{mpsc, Semaphore};
use tracing::{debug, error, info, warn};

use super::config::HarnessConfig;
use super::report::HarnessReport;
use super::task::{BenchTask, StreamResult};

/// Orchestrates concurrent benchmark execution with bounded parallelism.
pub struct HarnessRunner {
    config: HarnessConfig,
    semaphore: Arc<Semaphore>,
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

        let semaphore = Arc::new(Semaphore::new(config.max_concurrent));

        Ok(Self {
            config,
            semaphore,
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

        // Channel for collecting results
        let (tx, mut rx) = mpsc::channel::<StreamResult>(total_tasks.max(1));

        // Spawn all tasks — semaphore bounds actual concurrency
        let mut handles = Vec::with_capacity(total_tasks);
        for task in tasks {
            let permit_sem = self.semaphore.clone();
            let client = self.client.clone();
            let config = self.config.clone();
            let tx = tx.clone();
            let seq = self.sequence.clone();

            let handle = tokio::spawn(async move {
                let _permit = permit_sem
                    .acquire()
                    .await
                    .expect("semaphore closed unexpectedly");
                let stream_id = seq.fetch_add(1, Ordering::Relaxed) as usize;
                let result = execute_task(&client, &config, &task, stream_id).await;
                let _ = tx.send(result).await;
            });
            handles.push(handle);
        }

        // Drop our sender so rx completes when all tasks finish
        drop(tx);

        // Collect results
        let mut results = Vec::with_capacity(total_tasks);
        let mut completed = 0usize;
        while let Some(result) = rx.recv().await {
            completed += 1;
            let status = if result.success { "OK" } else { "FAIL" };
            let score_str = result
                .eval
                .as_ref()
                .map(|e| format!("{:.0}%", e.score * 100.0))
                .unwrap_or_else(|| "N/A".into());
            eprintln!(
                "  [{completed}/{total_tasks}] {} — {status} | {score_str} | {}ms | {}+{} tokens",
                result.task_id,
                result.latency_ms,
                result.prompt_tokens,
                result.completion_tokens,
            );
            results.push(result);
        }

        // Wait for all spawned tasks to complete
        for handle in handles {
            if let Err(e) = handle.await {
                warn!("Task join error: {e}");
            }
        }

        let total_duration = run_start.elapsed();

        let report = HarnessReport::from_results(
            &self.config,
            results,
            total_duration.as_secs_f64(),
        );

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

    let body = json!({
        "model": config.model,
        "messages": task.messages,
        "max_tokens": config.max_tokens,
        "temperature": config.temperature,
        "stream": false,
    });

    debug!(task_id = %task.id, stream_id, "Sending request");

    let response = match client.post(&url).json(&body).send().await {
        Ok(resp) => resp,
        Err(e) => {
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
                error: Some(format!("Request error: {e}")),
            };
        }
    };

    let status = response.status();
    let body_text = match response.text().await {
        Ok(t) => t,
        Err(e) => {
            return StreamResult {
                task_id: task.id.clone(),
                stream_id,
                success: false,
                response: String::new(),
                prompt_tokens: 0,
                completion_tokens: 0,
                latency_ms: start.elapsed().as_millis() as u64,
                eval: None,
                error: Some(format!("Body read error: {e}")),
            };
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
                error: Some(format!("JSON parse error: {e}")),
            };
        }
    };

    let content = parsed["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let prompt_tokens = parsed["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
    let completion_tokens = parsed["usage"]["completion_tokens"].as_u64().unwrap_or(0);

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
        response: content,
        prompt_tokens,
        completion_tokens,
        latency_ms,
        eval: Some(eval),
        error: None,
    }
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
        let mut config = HarnessConfig::default();
        config.endpoint = String::new();
        assert!(HarnessRunner::new(config).is_err());
    }
}
