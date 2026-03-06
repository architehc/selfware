//! VLM benchmark runner with semaphore-bounded concurrency.

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use base64::Engine;
use reqwest::Client;
use serde_json::json;
use tokio::sync::Semaphore;

use super::config::VlmBenchConfig;
use super::report::{BenchReport, LevelReport};
use super::scoring::{LevelScore, Rating};
use super::{BenchScenario, VlmBenchLevel};

/// Orchestrates VLM benchmark execution with bounded concurrency.
pub struct VlmBenchRunner {
    config: VlmBenchConfig,
    semaphore: Arc<Semaphore>,
    client: Client,
    levels: Vec<Box<dyn VlmBenchLevel>>,
}

impl VlmBenchRunner {
    /// Create a new runner from config and registered levels.
    pub fn new(config: VlmBenchConfig, levels: Vec<Box<dyn VlmBenchLevel>>) -> Result<Self> {
        config
            .validate()
            .map_err(|e| anyhow::anyhow!("Invalid config: {}", e))?;

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()
            .context("Failed to build HTTP client")?;

        let semaphore = Arc::new(Semaphore::new(config.max_concurrent));

        Ok(Self {
            config,
            semaphore,
            client,
            levels,
        })
    }

    /// Run all registered benchmark levels and produce a report.
    pub async fn run(&self) -> Result<BenchReport> {
        let run_start = Instant::now();
        let mut level_reports = Vec::new();

        let total_levels = self
            .levels
            .iter()
            .filter(|l| self.config.levels.contains(&l.difficulty()))
            .count();

        for level in &self.levels {
            // Skip levels not in the configured difficulty set
            if !self.config.levels.contains(&level.difficulty()) {
                continue;
            }

            eprintln!(
                "\n[{}/{}] Running {} ({:?})...",
                level_reports.len() + 1,
                total_levels,
                level.name(),
                level.difficulty()
            );
            let report = self.run_level(level.as_ref()).await?;
            eprintln!(
                "  => {}: {:.0}% {} | {} tokens | {:.1}s avg",
                level.name(),
                report.score * 100.0,
                report.rating,
                report.total_tokens,
                report.avg_latency_ms / 1000.0
            );
            level_reports.push(report);
        }

        let total_tokens: u64 = level_reports.iter().map(|r| r.total_tokens).sum();
        let overall_score = if level_reports.is_empty() {
            0.0
        } else {
            level_reports.iter().map(|r| r.score).sum::<f64>() / level_reports.len() as f64
        };
        let overall_rating = Rating::from_accuracy(overall_score, 0.6);
        let total_duration = run_start.elapsed();

        Ok(BenchReport {
            timestamp: chrono::Utc::now().to_rfc3339(),
            model: self.config.model.clone(),
            endpoint: self.config.endpoint.clone(),
            levels: level_reports,
            overall_score,
            overall_rating,
            total_tokens,
            total_duration_secs: total_duration.as_secs_f64(),
        })
    }

    /// Run a single benchmark level.
    async fn run_level(&self, level: &dyn VlmBenchLevel) -> Result<LevelReport> {
        let scenarios = level.scenarios();
        let scenario_count = scenarios.len();
        let mut scores = Vec::with_capacity(scenario_count);

        // Run scenarios with semaphore-bounded concurrency
        let mut handles = Vec::new();

        for (idx, scenario) in scenarios.iter().enumerate() {
            eprintln!(
                "  [{}/{}] {} ...",
                idx + 1,
                scenario_count,
                scenario.id
            );
            let permit = self
                .semaphore
                .clone()
                .acquire_owned()
                .await
                .context("Semaphore closed")?;
            let client = self.client.clone();
            let endpoint = self.config.endpoint.clone();
            let model = self.config.model.clone();
            let max_tokens = self.config.max_tokens;
            let temperature = self.config.temperature;
            let scenario = scenario.clone();

            let handle = tokio::spawn(async move {
                let start = Instant::now();
                let result = call_vlm(
                    &client,
                    &endpoint,
                    &model,
                    &scenario,
                    max_tokens,
                    temperature,
                )
                .await;
                let latency_ms = start.elapsed().as_millis() as u64;
                drop(permit);
                (scenario, result, latency_ms)
            });
            handles.push(handle);
        }

        for handle in handles {
            let (scenario, result, latency_ms) = handle.await.context("Task join failed")?;

            match result {
                Ok((response_text, token_count)) => {
                    let mut score = level.evaluate(&scenario, &response_text);
                    score.latency_ms = latency_ms;
                    score.response_tokens = token_count;
                    eprintln!(
                        "    {} => {:.0}% {} | {} tok | {:.1}s",
                        scenario.id,
                        score.accuracy * 100.0,
                        score.rating,
                        token_count,
                        latency_ms as f64 / 1000.0
                    );
                    scores.push(score);
                }
                Err(e) => {
                    tracing::warn!("Scenario {} failed: {}", scenario.id, e);
                    eprintln!(
                        "    {} => FAILED ({:.1}s): {}",
                        scenario.id,
                        latency_ms as f64 / 1000.0,
                        e
                    );
                    scores.push(LevelScore {
                        accuracy: 0.0,
                        detail_scores: vec![("error".into(), 0.0)],
                        response_tokens: 0,
                        latency_ms,
                        rating: Rating::Frost,
                    });
                }
            }
        }

        let avg_score = if scores.is_empty() {
            0.0
        } else {
            scores.iter().map(|s| s.accuracy).sum::<f64>() / scores.len() as f64
        };
        let total_tokens: u64 = scores.iter().map(|s| s.response_tokens).sum();
        let avg_latency = if scores.is_empty() {
            0.0
        } else {
            scores.iter().map(|s| s.latency_ms as f64).sum::<f64>() / scores.len() as f64
        };
        let rating = Rating::from_accuracy(avg_score, pass_threshold_for(level.difficulty()));

        Ok(LevelReport {
            name: level.name().to_string(),
            difficulty: level.difficulty(),
            description: level.description().to_string(),
            scenario_count: scores.len(),
            score: avg_score,
            rating,
            total_tokens,
            avg_latency_ms: avg_latency,
            scores,
        })
    }
}

/// Call the VLM endpoint with an image + prompt.
///
/// Returns (response_text, token_count).
async fn call_vlm(
    client: &Client,
    endpoint: &str,
    model: &str,
    scenario: &BenchScenario,
    max_tokens: usize,
    temperature: f32,
) -> Result<(String, u64)> {
    // Read and encode the image
    let image_bytes = tokio::fs::read(&scenario.image_path)
        .await
        .with_context(|| {
            format!(
                "Failed to read fixture image: {}",
                scenario.image_path.display()
            )
        })?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&image_bytes);
    let data_uri = format!("data:image/png;base64,{}", b64);

    let body = json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": [
                { "type": "text", "text": scenario.prompt },
                { "type": "image_url", "image_url": { "url": data_uri, "detail": "high" } }
            ]
        }],
        "max_tokens": max_tokens,
        "temperature": temperature,
        "stream": false
    });

    let url = format!("{}/chat/completions", endpoint.trim_end_matches('/'));
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .with_context(|| format!("Failed to connect to VLM endpoint: {}", url))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "VLM API returned HTTP {}: {}",
            status.as_u16(),
            text.chars().take(500).collect::<String>()
        );
    }

    let json: serde_json::Value = response
        .json()
        .await
        .context("Failed to parse VLM response JSON")?;

    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();
    // Some thinking models put reasoning in a separate field — include it for keyword matching
    let reasoning = json["choices"][0]["message"]["reasoning_content"]
        .as_str()
        .unwrap_or("");
    let full_response = if reasoning.is_empty() {
        content
    } else {
        format!("{}\n{}", reasoning, content)
    };
    let tokens = json["usage"]["total_tokens"].as_u64().unwrap_or(0);

    Ok((full_response, tokens))
}

/// Get the pass threshold for a given difficulty level.
fn pass_threshold_for(difficulty: super::Difficulty) -> f64 {
    match difficulty {
        super::Difficulty::Easy => 0.80,
        super::Difficulty::Medium => 0.70,
        super::Difficulty::Hard => 0.60,
        super::Difficulty::VeryHard => 0.50,
        super::Difficulty::Extreme => 0.40,
        super::Difficulty::Mega => 0.50,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pass_thresholds() {
        assert!((pass_threshold_for(super::super::Difficulty::Easy) - 0.80).abs() < f64::EPSILON);
        assert!((pass_threshold_for(super::super::Difficulty::Medium) - 0.70).abs() < f64::EPSILON);
        assert!((pass_threshold_for(super::super::Difficulty::Hard) - 0.60).abs() < f64::EPSILON);
        assert!(
            (pass_threshold_for(super::super::Difficulty::VeryHard) - 0.50).abs() < f64::EPSILON
        );
        assert!(
            (pass_threshold_for(super::super::Difficulty::Extreme) - 0.40).abs() < f64::EPSILON
        );
        assert!((pass_threshold_for(super::super::Difficulty::Mega) - 0.50).abs() < f64::EPSILON);
    }

    #[test]
    fn test_runner_creation_validates_config() {
        let bad_config = VlmBenchConfig {
            endpoint: String::new(),
            ..VlmBenchConfig::default()
        };
        assert!(VlmBenchRunner::new(bad_config, vec![]).is_err());
    }

    #[test]
    fn test_runner_creation_ok() {
        let config = VlmBenchConfig::default();
        let runner = VlmBenchRunner::new(config, vec![]);
        assert!(runner.is_ok());
    }
}
