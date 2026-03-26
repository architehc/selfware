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

use std::future::Future;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde_json::json;

use selfware::api::types::Message;
use selfware::bench_harness::*;
use selfware::config::auto_config::AutoConfigurator;
use selfware::self_healing::{ErrorOccurrence, SelfHealingConfig, SelfHealingEngine};
use selfware::tools::vision::{encode_image_file, VisionCompare};
use selfware::tools::Tool;
use selfware::visual_verification::{VisualDiffResult, VisualVerifier};

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

    fn with_note(mut self, note: impl AsRef<str>) -> Self {
        let note = note.as_ref();
        if !note.is_empty() {
            self.details = format!("{} | {}", self.details, note);
        }
        self
    }
}

#[derive(Debug, Clone)]
struct EndpointTarget {
    endpoint: String,
    concurrent: usize,
}

fn default_targets() -> Vec<EndpointTarget> {
    vec![
        EndpointTarget {
            endpoint: "http://localhost:8000/v1".to_string(),
            concurrent: 16,
        },
        EndpointTarget {
            endpoint: "https://crazyshit.ngrok.io/v1".to_string(),
            concurrent: 64,
        },
    ]
}

fn is_local_endpoint(endpoint: &str) -> bool {
    endpoint.contains("localhost") || endpoint.contains("127.0.0.1") || endpoint.contains("[::1]")
}

fn infer_text_concurrency(endpoint: &str) -> usize {
    if is_local_endpoint(endpoint) {
        16
    } else {
        64
    }
}

fn infer_vision_concurrency(endpoint: &str) -> usize {
    if is_local_endpoint(endpoint) {
        4
    } else {
        8
    }
}

fn infer_vision_timeout(endpoint: &str) -> Duration {
    if is_local_endpoint(endpoint) {
        Duration::from_secs(30)
    } else {
        Duration::from_secs(45)
    }
}

fn sanitize_endpoint(endpoint: &str) -> String {
    endpoint
        .replace("://", "_")
        .replace(['/', ':', '.'], "_")
        .trim_matches('_')
        .to_string()
}

fn preview(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

fn classify_flow_errors(errors: &[String]) -> (&'static str, String) {
    let message = if errors.is_empty() {
        "unknown endpoint step failure".to_string()
    } else {
        errors.join(" | ")
    };
    let lower = message.to_lowercase();

    let error_type = if lower.contains("429") || lower.contains("rate limit") {
        "rate_limit"
    } else if lower.contains("timeout") || lower.contains("timed out") {
        "timeout"
    } else if lower.contains("connection")
        || lower.contains("network")
        || lower.contains("refused")
        || lower.contains("reset")
        || lower.contains("circuit breaker")
    {
        "network"
    } else if lower.contains("parse") || lower.contains("json") || lower.contains("invalid") {
        "parse"
    } else {
        "unknown"
    };

    (error_type, message)
}

async fn retry_after_self_heal<F, Fut>(
    engine: &SelfHealingEngine,
    endpoint: &str,
    step: &str,
    errors: &[String],
    run: F,
) -> Option<FlowResult>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = FlowResult>,
{
    let (error_type, message) = classify_flow_errors(errors);
    let context = format!("{step}@{endpoint}");
    let recovery = engine.handle_error(ErrorOccurrence::new(error_type, &message, &context))?;

    if recovery.success {
        Some(run().await)
    } else {
        None
    }
}

fn vision_fixture(name: &str) -> PathBuf {
    PathBuf::from("vlm_fixtures/l1_tui_state").join(name)
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
        "backend={}, model={}, context={}, fc={}, streaming={}, thinking={}, disable_thinking={}, text_concurrency={}, vision_concurrency={}",
        results
            .backend_type
            .map(|b| b.name())
            .unwrap_or("unknown"),
        model.id,
        model.max_model_len,
        results.function_calling,
        results.streaming,
        results.thinking_supported,
        results.thinking_eats_tokens,
        infer_text_concurrency(endpoint),
        infer_vision_concurrency(endpoint),
    );

    if errors.is_empty() {
        FlowResult::ok("auto-config", start.elapsed().as_secs_f64(), &details)
    } else {
        FlowResult::fail(
            "auto-config",
            start.elapsed().as_secs_f64(),
            &details,
            errors,
        )
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
        temperature: 0.2,
        timeout_secs: if concurrent > 32 { 180 } else { 120 },
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
        .filter(|r| {
            r.error
                .as_ref()
                .map(|e| e.contains("timeout"))
                .unwrap_or(false)
        })
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
                .map(|e| e.contains("connection") || e.contains("refused") || e.contains("reset"))
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
            eprintln!(
                "  [{}] WARN: empty response (tokens: {}+{})",
                r.task_id, r.prompt_tokens, r.completion_tokens
            );
        }
    }

    if errors.is_empty() {
        FlowResult::ok("throughput", start.elapsed().as_secs_f64(), &details)
    } else {
        FlowResult::fail(
            "throughput",
            start.elapsed().as_secs_f64(),
            &details,
            errors,
        )
    }
}

// ---- Step 3: Error resilience ----
async fn test_error_resilience(endpoint: &str, model: &str, burst_size: usize) -> FlowResult {
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
                errors.push(format!(
                    "Large prompt unexpected: HTTP {status}: {}",
                    &body[..body.len().min(200)]
                ));
            }
        }
        Err(e) => {
            errors.push(format!("Large prompt connection error: {e}"));
        }
    }

    // Test 3e: Concurrent burst (stress test)
    let burst_size = burst_size.max(4);
    eprintln!("  [3e] Testing concurrent burst ({burst_size} simultaneous)...");
    tests_run += 1;
    let mut handles = vec![];
    for i in 0..burst_size {
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
    if burst_fail > burst_size / 2 {
        // More than half failed
        errors.push(format!("Burst test: {burst_fail}/{burst_size} failed"));
    } else {
        eprintln!("    OK: {burst_ok}/{burst_size} succeeded in burst");
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
        FlowResult::fail(
            "tool-calling",
            start.elapsed().as_secs_f64(),
            &details,
            errors,
        )
    }
}

// ---- Step 5: Vision workflows ----
async fn test_vision_workflows(
    endpoint: &str,
    model: &str,
    detail: &str,
    max_tokens: usize,
) -> FlowResult {
    let start = Instant::now();
    let request_timeout = infer_vision_timeout(endpoint);
    eprintln!("\n=== Step 5: Vision Workflows (detail={detail}, max_tokens={max_tokens}) ===");

    let compare = VisionCompare;
    let mut errors = vec![];
    let mut tests_run = 0;
    let mut tests_passed = 0;
    let mut analyze_latency_secs = 0.0;
    let mut compare_latency_secs = 0.0;

    let dashboard_normal = vision_fixture("dashboard_normal.png");
    let help_panel = vision_fixture("help_panel.png");
    let dashboard_path = dashboard_normal.display().to_string();
    let help_panel_path = help_panel.display().to_string();

    let dashboard_base64 = match encode_image_file(&dashboard_path) {
        Ok(image) => image,
        Err(e) => {
            return FlowResult::fail(
                "vision-workflows",
                start.elapsed().as_secs_f64(),
                "Failed to load dashboard fixture",
                vec![e.to_string()],
            );
        }
    };
    let help_panel_base64 = match encode_image_file(&help_panel_path) {
        Ok(image) => image,
        Err(e) => {
            return FlowResult::fail(
                "vision-workflows",
                start.elapsed().as_secs_f64(),
                "Failed to load help-panel fixture",
                vec![e.to_string()],
            );
        }
    };

    let verifier_extra_body = json!({
        "chat_template_kwargs": { "enable_thinking": false }
    });
    let verifier = VisualVerifier::new(endpoint, model)
        .with_timeout(request_timeout.as_secs())
        .with_generation(max_tokens, 0.0)
        .with_image_detail(detail)
        .with_extra_body(verifier_extra_body.as_object().cloned());

    tests_run += 1;
    eprintln!("  [5a] Testing single-image analysis...");
    let analyze_start = Instant::now();
    match tokio::time::timeout(
        request_timeout,
        verifier.verify_screenshot(
            &help_panel_base64,
            "A terminal dashboard with a help or shortcut panel open and readable shortcut text visible inside the panel.",
        ),
    )
    .await
    {
        Ok(Ok(analysis)) => {
            analyze_latency_secs = analyze_start.elapsed().as_secs_f64();
            let informative = analysis.passed && !analysis.description.trim().is_empty();
            let preview_text = preview(&analysis.description, 100);

            if informative {
                eprintln!("    OK: {preview_text}");
                tests_passed += 1;
            } else {
                let issue_text = if analysis.issues.is_empty() {
                    "no issues reported".to_string()
                } else {
                    analysis.issues.join("; ")
                };
                errors.push(format!(
                    "Single-image analysis mismatch: passed={}, description={}, issues={}",
                    analysis.passed,
                    preview_text,
                    preview(&issue_text, 120),
                ));
            }
        }
        Ok(Err(e)) => errors.push(format!("Single-image analysis failed: {e}")),
        Err(_) => errors.push(format!(
            "Single-image analysis timed out after {}s",
            request_timeout.as_secs()
        )),
    }

    tests_run += 1;
    eprintln!("  [5b] Testing image comparison workflow...");
    let compare_start = Instant::now();
    match tokio::time::timeout(
        request_timeout,
        compare.execute(json!({
            "image_a": dashboard_path,
            "image_b": help_panel_path,
            "threshold": 99.9,
        })),
    )
    .await
    {
        Ok(Ok(value)) => {
            compare_latency_secs = compare_start.elapsed().as_secs_f64();
            let pixel_similarity = value["pixel_similarity"].as_f64().unwrap_or(0.0);
            let semantic = match tokio::time::timeout(
                request_timeout,
                verifier.compare_screenshots(
                    &dashboard_base64,
                    &help_panel_base64,
                    "Image 2 should show a help or shortcut panel opened over the dashboard with additional visible shortcut text.",
                ),
            )
            .await
            {
                Ok(Ok(result)) => result,
                Ok(Err(e)) => {
                    errors.push(format!("Structured semantic compare failed: {e}"));
                    VisualDiffResult {
                        changes_detected: false,
                        expected_change_found: false,
                        description: String::new(),
                        unexpected_changes: vec![],
                    }
                }
                Err(_) => {
                    errors.push(format!(
                        "Structured semantic compare timed out after {}s",
                        request_timeout.as_secs()
                    ));
                    VisualDiffResult {
                        changes_detected: false,
                        expected_change_found: false,
                        description: String::new(),
                        unexpected_changes: vec![],
                    }
                }
            };
            let semantic_ok = semantic.changes_detected && semantic.expected_change_found;
            let semantic_preview = preview(&semantic.description, 120);
            let unexpected = if semantic.unexpected_changes.is_empty() {
                String::new()
            } else {
                format!(
                    ", unexpected={}",
                    preview(&semantic.unexpected_changes.join("; "), 120)
                )
            };

            if pixel_similarity < 99.9 && semantic_ok {
                eprintln!(
                    "    OK: pixel_similarity={:.1}, semantic={}",
                    pixel_similarity, semantic_preview
                );
                tests_passed += 1;
            } else {
                errors.push(format!(
                    "Image comparison too weak: pixel_similarity={:.1}, semantic={}{}",
                    pixel_similarity, semantic_preview, unexpected
                ));
            }
        }
        Ok(Err(e)) => errors.push(format!("Image comparison failed: {e}")),
        Err(_) => errors.push(format!(
            "Image comparison timed out after {}s",
            request_timeout.as_secs()
        )),
    }

    let details = format!(
        "{tests_passed}/{tests_run} vision checks passed, detail={detail}, max_tokens={max_tokens}, timeout={}s, analyze={:.1}s, compare={:.1}s",
        request_timeout.as_secs(),
        analyze_latency_secs,
        compare_latency_secs,
    );

    if errors.is_empty() {
        FlowResult::ok("vision-workflows", start.elapsed().as_secs_f64(), &details)
    } else {
        FlowResult::fail(
            "vision-workflows",
            start.elapsed().as_secs_f64(),
            &details,
            errors,
        )
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    eprintln!("=== Full Flow Integration Test ===");
    std::fs::create_dir_all("bench_results/flow_test")?;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut endpoint_arg: Option<String> = None;
    let mut concurrent_arg: Option<usize> = None;
    let mut vision_only = false;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--concurrent" => {
                if let Some(value) = args.get(i + 1).and_then(|s| s.parse().ok()) {
                    concurrent_arg = Some(value);
                }
                i += 2;
            }
            "--vision-only" => {
                vision_only = true;
                i += 1;
            }
            arg if arg.starts_with("--") => {
                i += 1;
            }
            _ => {
                if endpoint_arg.is_none() {
                    endpoint_arg = Some(args[i].clone());
                }
                i += 1;
            }
        }
    }

    let targets = if endpoint_arg.is_none() && concurrent_arg.is_none() && !vision_only {
        default_targets()
    } else {
        let endpoint = endpoint_arg.unwrap_or_else(|| "https://crazyshit.ngrok.io/v1".to_string());
        let concurrent = concurrent_arg.unwrap_or_else(|| infer_text_concurrency(&endpoint));
        vec![EndpointTarget {
            endpoint,
            concurrent,
        }]
    };

    let mut combined_reports = Vec::new();
    let mut any_failed = false;

    for target in targets {
        eprintln!("\n{}", "=".repeat(70));
        eprintln!(
            "Endpoint: {} | Text concurrency: {} | Vision concurrency: {}",
            target.endpoint,
            target.concurrent,
            infer_vision_concurrency(&target.endpoint),
        );

        let total_start = Instant::now();
        let healer = SelfHealingEngine::new(SelfHealingConfig {
            max_healing_attempts: 3,
            ..SelfHealingConfig::default()
        });
        let mut results = vec![];

        // Step 1: Auto-config
        results.push(test_auto_config(&target.endpoint).await);

        // Get model name for subsequent tests
        let configurator = AutoConfigurator::new(&target.endpoint, None);
        let model = configurator
            .fetch_models()
            .await
            .ok()
            .and_then(|m| m.first().map(|m| m.id.clone()))
            .unwrap_or_else(|| "unknown".to_string());

        let _ = healer.checkpoint(
            "endpoint-calibration",
            json!({
                "endpoint": target.endpoint,
                "model": model,
                "text_concurrency": target.concurrent,
                "vision_concurrency": infer_vision_concurrency(&target.endpoint),
            }),
        );

        if vision_only {
            let vision = test_vision_workflows(&target.endpoint, &model, "low", 192).await;
            let vision = if vision.passed {
                vision
            } else {
                match retry_after_self_heal(
                    &healer,
                    &target.endpoint,
                    "vision-workflows",
                    &vision.errors,
                    || async {
                        test_vision_workflows(&target.endpoint, &model, "high", 256)
                            .await
                            .with_note("self-heal retry switched to detail=high max_tokens=256")
                    },
                )
                .await
                {
                    Some(recovered) => recovered,
                    None => vision.with_note("self-heal exhausted"),
                }
            };
            results.push(vision);
        } else {
            // Step 2: Throughput with self-heal fallback
            let throughput = test_throughput(&target.endpoint, &model, target.concurrent).await;
            let throughput = if throughput.passed {
                throughput
            } else {
                let fallback_concurrent = (target.concurrent / 2).max(4);
                match retry_after_self_heal(
                    &healer,
                    &target.endpoint,
                    "throughput",
                    &throughput.errors,
                    || async {
                        test_throughput(&target.endpoint, &model, fallback_concurrent)
                            .await
                            .with_note(format!(
                                "self-heal retry reduced concurrency to {fallback_concurrent}"
                            ))
                    },
                )
                .await
                {
                    Some(recovered) => recovered,
                    None => throughput.with_note("self-heal exhausted"),
                }
            };
            results.push(throughput);

            // Step 3: Vision workflows with self-heal fallback
            let vision = test_vision_workflows(&target.endpoint, &model, "low", 192).await;
            let vision = if vision.passed {
                vision
            } else {
                match retry_after_self_heal(
                    &healer,
                    &target.endpoint,
                    "vision-workflows",
                    &vision.errors,
                    || async {
                        test_vision_workflows(&target.endpoint, &model, "high", 256)
                            .await
                            .with_note("self-heal retry switched to detail=high max_tokens=256")
                    },
                )
                .await
                {
                    Some(recovered) => recovered,
                    None => vision.with_note("self-heal exhausted"),
                }
            };
            results.push(vision);

            // Step 4: Error resilience with self-heal retry
            let error_resilience =
                test_error_resilience(&target.endpoint, &model, target.concurrent).await;
            let error_resilience = if error_resilience.passed {
                error_resilience
            } else {
                match retry_after_self_heal(
                    &healer,
                    &target.endpoint,
                    "error-resilience",
                    &error_resilience.errors,
                    || async {
                        test_error_resilience(&target.endpoint, &model, target.concurrent)
                            .await
                            .with_note("self-heal retry reran resilience checks")
                    },
                )
                .await
                {
                    Some(recovered) => recovered,
                    None => error_resilience.with_note("self-heal exhausted"),
                }
            };
            results.push(error_resilience);

            // Step 5: Tool calling with self-heal retry
            let tool_calling = test_tool_calling(&target.endpoint, &model).await;
            let tool_calling = if tool_calling.passed {
                tool_calling
            } else {
                match retry_after_self_heal(
                    &healer,
                    &target.endpoint,
                    "tool-calling",
                    &tool_calling.errors,
                    || async {
                        test_tool_calling(&target.endpoint, &model)
                            .await
                            .with_note("self-heal retry reran tool-calling checks")
                    },
                )
                .await
                {
                    Some(recovered) => recovered,
                    None => tool_calling.with_note("self-heal exhausted"),
                }
            };
            results.push(tool_calling);
        }

        // Print summary
        let total_duration = total_start.elapsed().as_secs_f64();
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = results.iter().filter(|r| !r.passed).count();
        let heal_summary = healer.summary();

        eprintln!("\n{}", "=".repeat(70));
        eprintln!("FULL FLOW TEST RESULTS — {}", target.endpoint);
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
        eprintln!(
            "  self-heal: executions={}, successes={}, failures={}",
            heal_summary.executor.executions,
            heal_summary.executor.successes,
            heal_summary.executor.failures,
        );
        eprintln!("{}", "=".repeat(70));

        let report = json!({
            "endpoint": target.endpoint,
            "model": model,
            "text_concurrency": target.concurrent,
            "vision_concurrency": infer_vision_concurrency(&target.endpoint),
            "vision_only": vision_only,
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
            "self_healing": {
                "executions": heal_summary.executor.executions,
                "successes": heal_summary.executor.successes,
                "failures": heal_summary.executor.failures,
                "success_rate": heal_summary.executor.success_rate,
            }
        });

        let report_path = format!(
            "bench_results/flow_test/report_{}.json",
            sanitize_endpoint(&target.endpoint)
        );
        std::fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;
        eprintln!("\nReport saved to {report_path}");

        any_failed |= failed > 0;
        combined_reports.push(report);
    }

    std::fs::write(
        "bench_results/flow_test/report.json",
        serde_json::to_string_pretty(&combined_reports)?,
    )?;
    eprintln!("\nCombined report saved to bench_results/flow_test/report.json");

    if any_failed {
        std::process::exit(1);
    }

    Ok(())
}
