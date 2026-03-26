//! Full flow integration test — exercises the complete user journey:
//!
//! 1. Auto-config endpoint detection
//! 2. Throughput benchmark (16 concurrent)
//! 3. SWE-bench agent loop (3 tasks)
//! 4. Error detection and self-healing validation
//!
//! Tests against a live endpoint and reports any unexpected errors.
//!
//! Run with:
//!   cargo run --features bench-harness --example full_flow_test -- https://crazyshit.ngrok.io/v1

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use serde_json::json;

use selfware::api::types::Message;
use selfware::bench_harness::*;
use selfware::config::auto_config::AutoConfigurator;

#[derive(Debug)]
struct FlowResult {
    step: String,
    passed: bool,
    duration_secs: f64,
    details: String,
    errors: Vec<String>,
}

impl FlowResult {
    fn ok(step: &str, duration: f64, details: &str) -> Self {
        Self {
            step: step.to_string(),
            passed: true,
            duration_secs: duration,
            details: details.to_string(),
            errors: vec![],
        }
    }

    fn fail(step: &str, duration: f64, details: &str, errors: Vec<String>) -> Self {
        Self {
            step: step.to_string(),
            passed: false,
            duration_secs: duration,
            details: details.to_string(),
            errors,
        }
    }
}

// ---- Step 1: Auto-config ----
async fn test_auto_config(endpoint: &str) -> FlowResult {
    let start = Instant::now();
    eprintln!("\n=== Step 1: Auto-Configuration ===");

    let configurator = AutoConfigurator::new(endpoint, None);

    // Fetch models
    let models = match configurator.fetch_models().await {
        Ok(m) => m,
        Err(e) => {
            return FlowResult::fail(
                "auto-config",
                start.elapsed().as_secs_f64(),
                "Failed to fetch models",
                vec![format!("{e}")],
            );
        }
    };

    if models.is_empty() {
        return FlowResult::fail(
            "auto-config",
            start.elapsed().as_secs_f64(),
            "No models found",
            vec!["Endpoint returned empty model list".into()],
        );
    }

    let model = &models[0];
    eprintln!("  Model: {} ({})", model.id, model.owned_by);
    eprintln!("  Context: {} tokens", model.max_model_len);

    // Run detection tests
    let results = match configurator.run_tests(&model.id).await {
        Ok(r) => r,
        Err(e) => {
            return FlowResult::fail(
                "auto-config",
                start.elapsed().as_secs_f64(),
                "Detection tests failed",
                vec![format!("{e}")],
            );
        }
    };

    let mut errors = vec![];
    if !results.chat_works {
        errors.push("Chat API not working".into());
    }

    let details = format!(
        "backend={}, model={}, context={}, fc={}, streaming={}, thinking={}",
        results
            .backend_type
            .map(|b| b.name())
            .unwrap_or("unknown"),
        model.id,
        model.max_model_len,
        results.function_calling,
        results.streaming,
        results.thinking_supported,
    );

    if errors.is_empty() {
        FlowResult::ok("auto-config", start.elapsed().as_secs_f64(), &details)
    } else {
        FlowResult::fail("auto-config", start.elapsed().as_secs_f64(), &details, errors)
    }
}

// ---- Step 2: Throughput benchmark ----
async fn test_throughput(endpoint: &str, model: &str, concurrent: usize) -> FlowResult {
    let start = Instant::now();
    eprintln!("\n=== Step 2: Throughput Benchmark ({concurrent} concurrent) ===");

    let config = HarnessConfig {
        endpoint: endpoint.to_string(),
        model: model.to_string(),
        max_concurrent: concurrent,
        max_tokens: 256,
        temperature: 0.7,
        timeout_secs: 120,
        output_dir: "bench_results/flow_test".into(),
        extra_body: json!({"chat_template_kwargs": {"enable_thinking": false}}),
    };

    let runner = match HarnessRunner::new(config) {
        Ok(r) => r,
        Err(e) => {
            return FlowResult::fail(
                "throughput",
                start.elapsed().as_secs_f64(),
                "Runner creation failed",
                vec![format!("{e}")],
            );
        }
    };

    // Generate tasks
    let tasks: Vec<BenchTask> = (0..concurrent)
        .map(|i| {
            let prompts = [
                "What is 2+2? Answer with just the number.",
                "Name the capital of France in one word.",
                "Is Rust a compiled language? Yes or no.",
                "What color is the sky? One word.",
            ];
            BenchTask {
                id: format!("flow-{i}"),
                description: format!("Quick test {i}"),
                messages: vec![
                    Message::system("Answer concisely in one sentence."),
                    Message::user(prompts[i % prompts.len()]),
                ],
                evaluator: Box::new(NoopEvaluator),
            }
        })
        .collect();

    let report = match runner.run(tasks).await {
        Ok(r) => r,
        Err(e) => {
            return FlowResult::fail(
                "throughput",
                start.elapsed().as_secs_f64(),
                "Benchmark run failed",
                vec![format!("{e}")],
            );
        }
    };

    let mut errors = vec![];

    // Check for unexpected failures
    if report.error_rate > 0.5 {
        errors.push(format!(
            "High error rate: {:.0}% ({} failed)",
            report.error_rate * 100.0,
            report.tasks_failed
        ));
    }

    // Check for timeouts
    let timeouts = report
        .results
        .iter()
        .filter(|r| r.error.as_ref().map(|e| e.contains("timeout")).unwrap_or(false))
        .count();
    if timeouts > 0 {
        errors.push(format!("{timeouts} tasks timed out"));
    }

    // Check for empty responses (thinking mode issue)
    let empty_responses = report
        .results
        .iter()
        .filter(|r| r.response.is_empty() && r.error.is_none())
        .count();
    if empty_responses > 0 {
        errors.push(format!(
            "{empty_responses} empty responses (likely thinking mode eating tokens)"
        ));
    }

    // Check for connection errors
    let conn_errors = report
        .results
        .iter()
        .filter(|r| {
            r.error
                .as_ref()
                .map(|e| {
                    e.contains("connection")
                        || e.contains("refused")
                        || e.contains("reset")
                })
                .unwrap_or(false)
        })
        .count();
    if conn_errors > 0 {
        errors.push(format!("{conn_errors} connection errors"));
    }

    let details = format!(
        "passed={}/{}, tok/s={:.0}, p50={:.1}s, p95={:.1}s, errors={}",
        report.tasks_passed,
        report.tasks_total,
        report.tokens_per_sec,
        report.latency_p50_ms as f64 / 1000.0,
        report.latency_p95_ms as f64 / 1000.0,
        report.tasks_failed,
    );

    // Print per-task errors for debugging
    for r in &report.results {
        if let Some(err) = &r.error {
            eprintln!("  [{}] ERROR: {}", r.task_id, &err[..err.len().min(100)]);
        } else if r.response.is_empty() {
            eprintln!("  [{}] WARN: empty response (tokens: {}+{})", r.task_id, r.prompt_tokens, r.completion_tokens);
        }
    }

    if errors.is_empty() {
        FlowResult::ok("throughput", start.elapsed().as_secs_f64(), &details)
    } else {
        FlowResult::fail("throughput", start.elapsed().as_secs_f64(), &details, errors)
    }
}

// ---- Step 3: Error resilience ----
async fn test_error_resilience(endpoint: &str, model: &str) -> FlowResult {
    let start = Instant::now();
    eprintln!("\n=== Step 3: Error Resilience ===");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();

    let mut errors = vec![];
    let mut tests_run = 0;
    let mut tests_passed = 0;

    // Test 3a: Invalid model name
    eprintln!("  [3a] Testing invalid model name...");
    tests_run += 1;
    let resp = client
        .post(&format!("{}/chat/completions", endpoint))
        .json(&json!({
            "model": "nonexistent-model-12345",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 16,
        }))
        .send()
        .await;
    match resp {
        Ok(r) => {
            if r.status().is_success() {
                errors.push("Invalid model accepted (should return error)".into());
            } else {
                eprintln!("    OK: HTTP {} for invalid model", r.status());
                tests_passed += 1;
            }
        }
        Err(e) => {
            errors.push(format!("Connection error on invalid model test: {e}"));
        }
    }

    // Test 3b: Empty messages
    eprintln!("  [3b] Testing empty messages...");
    tests_run += 1;
    let resp = client
        .post(&format!("{}/chat/completions", endpoint))
        .json(&json!({
            "model": model,
            "messages": [],
            "max_tokens": 16,
        }))
        .send()
        .await;
    match resp {
        Ok(r) => {
            let status = r.status();
            if status.is_server_error() {
                errors.push(format!("Server error on empty messages: HTTP {status}"));
            } else {
                eprintln!("    OK: HTTP {} for empty messages", status);
                tests_passed += 1;
            }
        }
        Err(e) => {
            errors.push(format!("Connection error on empty messages: {e}"));
        }
    }

    // Test 3c: max_tokens=1 (minimal response)
    eprintln!("  [3c] Testing max_tokens=1...");
    tests_run += 1;
    let resp = client
        .post(&format!("{}/chat/completions", endpoint))
        .json(&json!({
            "model": model,
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 1,
            "chat_template_kwargs": {"enable_thinking": false},
        }))
        .send()
        .await;
    match resp {
        Ok(r) => {
            if r.status().is_success() {
                eprintln!("    OK: max_tokens=1 handled gracefully");
                tests_passed += 1;
            } else {
                errors.push(format!("max_tokens=1 failed: HTTP {}", r.status()));
            }
        }
        Err(e) => {
            errors.push(format!("max_tokens=1 connection error: {e}"));
        }
    }

    // Test 3d: Very long prompt (near context limit)
    eprintln!("  [3d] Testing large prompt (50K tokens)...");
    tests_run += 1;
    let large_prompt = "Repeat this word: hello. ".repeat(10000); // ~50K tokens
    let resp = client
        .post(&format!("{}/chat/completions", endpoint))
        .json(&json!({
            "model": model,
            "messages": [{"role": "user", "content": large_prompt}],
            "max_tokens": 16,
            "chat_template_kwargs": {"enable_thinking": false},
        }))
        .send()
        .await;
    match resp {
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            if status.is_success() || status.as_u16() == 400 {
                // 400 is acceptable (prompt too long)
                eprintln!("    OK: HTTP {} for large prompt", status);
                tests_passed += 1;
            } else {
                errors.push(format!("Large prompt unexpected: HTTP {status}: {}", &body[..body.len().min(200)]));
            }
        }
        Err(e) => {
            errors.push(format!("Large prompt connection error: {e}"));
        }
    }

    // Test 3e: Concurrent burst (stress test)
    eprintln!("  [3e] Testing concurrent burst (32 simultaneous)...");
    tests_run += 1;
    let mut handles = vec![];
    for i in 0..32 {
        let c = client.clone();
        let url = format!("{}/chat/completions", endpoint);
        let m = model.to_string();
        handles.push(tokio::spawn(async move {
            c.post(&url)
                .json(&json!({
                    "model": m,
                    "messages": [{"role": "user", "content": format!("Say {i}")}],
                    "max_tokens": 16,
                    "chat_template_kwargs": {"enable_thinking": false},
                }))
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false)
        }));
    }

    let mut burst_ok = 0;
    let mut burst_fail = 0;
    for h in handles {
        match h.await {
            Ok(true) => burst_ok += 1,
            _ => burst_fail += 1,
        }
    }
    if burst_fail > 16 {
        // More than half failed
        errors.push(format!("Burst test: {burst_fail}/32 failed"));
    } else {
        eprintln!("    OK: {burst_ok}/32 succeeded in burst");
        tests_passed += 1;
    }

    let details = format!("{tests_passed}/{tests_run} resilience tests passed");

    if errors.is_empty() {
        FlowResult::ok("error-resilience", start.elapsed().as_secs_f64(), &details)
    } else {
        FlowResult::fail(
            "error-resilience",
            start.elapsed().as_secs_f64(),
            &details,
            errors,
        )
    }
}

// ---- Step 4: Tool calling validation ----
async fn test_tool_calling(endpoint: &str, model: &str) -> FlowResult {
    let start = Instant::now();
    eprintln!("\n=== Step 4: Tool Calling Validation ===");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .unwrap();

    let mut errors = vec![];

    // Test with tools + thinking disabled
    eprintln!("  [4a] Testing tool calling (thinking disabled)...");
    let resp = client
        .post(&format!("{}/chat/completions", endpoint))
        .json(&json!({
            "model": model,
            "messages": [{"role": "user", "content": "Read the file README.md"}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "file_read",
                    "description": "Read a file from disk",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string", "description": "File path to read"}
                        },
                        "required": ["path"]
                    }
                }
            }],
            "max_tokens": 512,
            "temperature": 0.0,
            "chat_template_kwargs": {"enable_thinking": false},
        }))
        .send()
        .await;

    match resp {
        Ok(r) => {
            let status = r.status();
            let body: serde_json::Value = r.json().await.unwrap_or(json!({}));

            if !status.is_success() {
                errors.push(format!("Tool calling HTTP {status}"));
            } else {
                let tool_calls = body["choices"][0]["message"]["tool_calls"]
                    .as_array()
                    .map(|a| a.len())
                    .unwrap_or(0);
                let content = body["choices"][0]["message"]["content"]
                    .as_str()
                    .unwrap_or("");
                let has_xml_tool = content.contains("<tool") || content.contains("file_read");

                if tool_calls > 0 {
                    eprintln!("    OK: Native FC returned {} tool_calls", tool_calls);
                } else if has_xml_tool {
                    eprintln!("    OK: Model outputs tool calls in text (XML format)");
                } else {
                    errors.push(format!(
                        "No tool calls in response (native={}, content={}chars)",
                        tool_calls,
                        content.len()
                    ));
                }
            }
        }
        Err(e) => {
            errors.push(format!("Tool calling request failed: {e}"));
        }
    }

    // Test with tools + thinking enabled
    eprintln!("  [4b] Testing tool calling (thinking enabled)...");
    let resp = client
        .post(&format!("{}/chat/completions", endpoint))
        .json(&json!({
            "model": model,
            "messages": [{"role": "user", "content": "Read the file README.md"}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "file_read",
                    "description": "Read a file",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"}
                        },
                        "required": ["path"]
                    }
                }
            }],
            "max_tokens": 1024,
            "temperature": 0.0,
        }))
        .send()
        .await;

    match resp {
        Ok(r) => {
            let status = r.status();
            let body: serde_json::Value = r.json().await.unwrap_or(json!({}));

            if !status.is_success() {
                errors.push(format!("Tool calling with thinking HTTP {status}"));
            } else {
                let tool_calls = body["choices"][0]["message"]["tool_calls"]
                    .as_array()
                    .map(|a| a.len())
                    .unwrap_or(0);
                let content = body["choices"][0]["message"]["content"]
                    .as_str()
                    .unwrap_or("");
                let reasoning = body["choices"][0]["message"]["reasoning_content"]
                    .as_str()
                    .or(body["choices"][0]["message"]["reasoning"].as_str());
                let content_null = content.is_empty();

                eprintln!(
                    "    Thinking: reasoning={}chars, content={}chars, tool_calls={}, content_null={}",
                    reasoning.map(|r| r.len()).unwrap_or(0),
                    content.len(),
                    tool_calls,
                    content_null,
                );

                if content_null && tool_calls == 0 && reasoning.is_some() {
                    errors.push("Thinking mode: content is null AND no tool_calls (all tokens consumed by reasoning)".into());
                }
            }
        }
        Err(e) => {
            errors.push(format!("Tool calling with thinking failed: {e}"));
        }
    }

    let details = format!("{} issues found", errors.len());
    if errors.is_empty() {
        FlowResult::ok("tool-calling", start.elapsed().as_secs_f64(), &details)
    } else {
        FlowResult::fail("tool-calling", start.elapsed().as_secs_f64(), &details, errors)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let endpoint = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "https://crazyshit.ngrok.io/v1".to_string());

    let concurrent: usize = std::env::args()
        .skip_while(|a| a != "--concurrent")
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(16);

    eprintln!("=== Full Flow Integration Test ===");
    eprintln!("Endpoint: {endpoint}");
    eprintln!("Concurrency: {concurrent}");

    let total_start = Instant::now();
    let mut results = vec![];

    // Step 1: Auto-config
    results.push(test_auto_config(&endpoint).await);

    // Get model name for subsequent tests
    let configurator = AutoConfigurator::new(&endpoint, None);
    let model = configurator
        .fetch_models()
        .await
        .ok()
        .and_then(|m| m.first().map(|m| m.id.clone()))
        .unwrap_or_else(|| "unknown".to_string());

    // Step 2: Throughput
    results.push(test_throughput(&endpoint, &model, concurrent).await);

    // Step 3: Error resilience
    results.push(test_error_resilience(&endpoint, &model).await);

    // Step 4: Tool calling
    results.push(test_tool_calling(&endpoint, &model).await);

    // Print summary
    let total_duration = total_start.elapsed().as_secs_f64();
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.iter().filter(|r| !r.passed).count();

    eprintln!("\n{}", "=".repeat(70));
    eprintln!("FULL FLOW TEST RESULTS — {endpoint}");
    eprintln!("{}", "=".repeat(70));
    eprintln!();

    for r in &results {
        let icon = if r.passed { "PASS" } else { "FAIL" };
        eprintln!(
            "  {} [{icon}] {} ({:.1}s)",
            if r.passed { "+" } else { "-" },
            r.step,
            r.duration_secs,
        );
        eprintln!("       {}", r.details);
        for err in &r.errors {
            eprintln!("       ! {err}");
        }
    }

    eprintln!();
    eprintln!(
        "  {passed}/{} passed, {failed} failed, {:.1}s total",
        results.len(),
        total_duration
    );
    eprintln!("{}", "=".repeat(70));

    // Save report
    let report = json!({
        "endpoint": endpoint,
        "model": model,
        "concurrent": concurrent,
        "total_duration_secs": total_duration,
        "passed": passed,
        "failed": failed,
        "results": results.iter().map(|r| json!({
            "step": r.step,
            "passed": r.passed,
            "duration_secs": r.duration_secs,
            "details": r.details,
            "errors": r.errors,
        })).collect::<Vec<_>>(),
    });

    std::fs::create_dir_all("bench_results/flow_test")?;
    std::fs::write(
        "bench_results/flow_test/report.json",
        serde_json::to_string_pretty(&report)?,
    )?;
    eprintln!("\nReport saved to bench_results/flow_test/report.json");

    if failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}
