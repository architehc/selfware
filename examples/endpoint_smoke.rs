//! Endpoint Smoke Test — exercises a real LLM endpoint end-to-end.
//!
//! This is the canonical "is my endpoint actually working with selfware" tool.
//! Each capability is probed independently and reported with pass/fail + timing
//! so a failing backend shows you exactly *which* feature is broken.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example endpoint_smoke -- \
//!     --endpoint http://127.0.0.1:8000/v1 \
//!     --model qwen3.5-27b
//! ```
//!
//! Flags:
//! - `--endpoint URL`  Base URL (default: `http://127.0.0.1:8000/v1`)
//! - `--model NAME`    Model id (default: auto-detect from `/v1/models`)
//! - `--api-key KEY`   Optional bearer token
//! - `--image PATH`    Optional image for the multimodal probe
//!
//! Exit code is `0` if every check passes, `1` otherwise.
//!
//! # Checks
//!
//! 1. Endpoint reachable (`GET /v1/models`)
//! 2. Backend classification (vLLM / SGLang / Ollama / LM Studio / unknown)
//! 3. Plain chat — "reply with `pong`"
//! 4. Streaming chat — same prompt, verify chunks + final text
//! 5. Tool call — calculator tool, "what is 17 * 23?"
//! 6. Multi-turn tool execution — feed result back, expect 391
//! 7. Thinking parsing — `chat_template_kwargs.enable_thinking=true`,
//!    verify `reasoning_content` is parsed without leaking into content
//! 8. Multimodal probe (only if `--image` is provided)

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use base64::Engine;
use clap::Parser;
use reqwest::Client as HttpClient;
use serde_json::{json, Value};

use selfware::api::client::RetryConfig;
use selfware::api::types::{
    ContentBlock, FunctionDefinition, ImageUrl, Message, MessageContent, ToolDefinition,
};
use selfware::api::{ApiClient, StreamChunk, ThinkingMode};
use selfware::config::{AgentConfig, Config, RedactedString};

const PER_CHECK_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Parser, Debug)]
#[command(
    name = "endpoint_smoke",
    about = "Probe an OpenAI-compatible LLM endpoint for selfware capability support",
    long_about = "Runs a fixed sequence of capability checks (chat, streaming, tools, \
                  multi-turn tool execution, thinking parsing, optional vision) against \
                  a live endpoint. Exits 0 if all pass, 1 if any fail."
)]
struct Cli {
    /// Endpoint base URL (must include the `/v1` path segment).
    #[arg(long, default_value = "http://127.0.0.1:8000/v1")]
    endpoint: String,

    /// Model id. If omitted, the first model from `/v1/models` is used.
    #[arg(long)]
    model: Option<String>,

    /// Optional bearer token sent as `Authorization: Bearer <key>`.
    #[arg(long)]
    api_key: Option<String>,

    /// Optional path to an image (PNG/JPEG) for the multimodal probe.
    #[arg(long)]
    image: Option<String>,
}

/// One pass/fail line of output, accumulated and replayed in the summary.
struct CheckResult {
    name: &'static str,
    passed: bool,
    timing_ms: u128,
    note: String,
}

impl CheckResult {
    fn pass(name: &'static str, timing_ms: u128, note: impl Into<String>) -> Self {
        Self {
            name,
            passed: true,
            timing_ms,
            note: note.into(),
        }
    }

    fn fail(name: &'static str, timing_ms: u128, note: impl Into<String>) -> Self {
        Self {
            name,
            passed: false,
            timing_ms,
            note: note.into(),
        }
    }

    fn print(&self) {
        let tag = if self.passed { "PASS" } else { "FAIL" };
        println!(
            "[{}]  {:<28}  {:>6}ms  {}",
            tag, self.name, self.timing_ms, self.note
        );
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("=== selfware endpoint smoke test ===");
    println!("endpoint: {}", cli.endpoint);
    if let Some(ref m) = cli.model {
        println!("model:    {} (user-supplied)", m);
    } else {
        println!("model:    <auto-detect>");
    }
    if cli.api_key.is_some() {
        println!("api_key:  [provided]");
    }
    if let Some(ref img) = cli.image {
        println!("image:    {}", img);
    }
    println!();

    let http = HttpClient::builder()
        .timeout(PER_CHECK_TIMEOUT)
        .connect_timeout(Duration::from_secs(10))
        .build()
        .context("failed to build smoke-test http client")?;

    let mut results: Vec<CheckResult> = Vec::new();

    // ── 1. Endpoint reachable ────────────────────────────────────────────────
    let (models_body, models_headers, reachable) =
        check_reachable(&http, &cli.endpoint, cli.api_key.as_deref()).await;
    results.push(reachable);

    // Resolve the model id once for the remaining checks.
    let resolved_model = match cli.model.clone() {
        Some(m) => Some(m),
        None => first_model_id(&models_body),
    };

    // ── 2. Backend classification ────────────────────────────────────────────
    results.push(
        check_backend(
            &http,
            &cli.endpoint,
            models_body.as_ref(),
            models_headers.as_ref(),
        )
        .await,
    );

    // The remaining checks all need a model id. If we never resolved one,
    // mark them skipped/failed and fall through to the summary.
    let Some(model) = resolved_model else {
        for name in [
            "plain_chat",
            "streaming",
            "tool_call",
            "tool_followup",
            "thinking_parse",
        ] {
            results.push(CheckResult::fail(
                box_static_str(name),
                0,
                "no model available — see endpoint_reachable",
            ));
        }
        if cli.image.is_some() {
            results.push(CheckResult::fail(
                "multimodal",
                0,
                "no model available — see endpoint_reachable",
            ));
        }
        return finish(results);
    };

    println!("(using model `{}`)", model);
    println!();

    // ── 3. Plain chat ────────────────────────────────────────────────────────
    results.push(
        with_timeout(
            "plain_chat",
            check_plain_chat(&cli.endpoint, &model, cli.api_key.as_deref()),
        )
        .await,
    );

    // ── 4. Streaming chat ────────────────────────────────────────────────────
    results.push(
        with_timeout(
            "streaming",
            check_streaming(&cli.endpoint, &model, cli.api_key.as_deref()),
        )
        .await,
    );

    // ── 5. Tool call ─────────────────────────────────────────────────────────
    let (tool_call_result, tool_call_msg) = with_timeout_and_value(
        "tool_call",
        check_tool_call(&cli.endpoint, &model, cli.api_key.as_deref()),
    )
    .await;
    results.push(tool_call_result);

    // ── 6. Multi-turn tool execution ─────────────────────────────────────────
    if let Some(assistant_msg) = tool_call_msg {
        results.push(
            with_timeout(
                "tool_followup",
                check_tool_followup(&cli.endpoint, &model, cli.api_key.as_deref(), assistant_msg),
            )
            .await,
        );
    } else {
        results.push(CheckResult::fail(
            "tool_followup",
            0,
            "skipped: tool_call did not produce a tool call to feed back",
        ));
    }

    // ── 7. Thinking parsing ──────────────────────────────────────────────────
    results.push(
        with_timeout(
            "thinking_parse",
            check_thinking(&cli.endpoint, &model, cli.api_key.as_deref()),
        )
        .await,
    );

    // ── 8. Multimodal (optional) ─────────────────────────────────────────────
    if let Some(ref image_path) = cli.image {
        results.push(
            with_timeout(
                "multimodal",
                check_multimodal(&cli.endpoint, &model, cli.api_key.as_deref(), image_path),
            )
            .await,
        );
    }

    finish(results)
}

/// Print summary and return the appropriate exit code.
fn finish(results: Vec<CheckResult>) -> Result<()> {
    println!();
    let mut passed = 0usize;
    let mut failed = 0usize;
    for r in &results {
        r.print();
        if r.passed {
            passed += 1;
        } else {
            failed += 1;
        }
    }
    println!();
    println!("---");
    println!(
        "summary: {} passed, {} failed (of {} checks)",
        passed,
        failed,
        results.len()
    );
    if failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}

/// Return a `&'static str` for a runtime-known name. We only use this with
/// names already declared as `&'static str` literals at the call site.
fn box_static_str(name: &'static str) -> &'static str {
    name
}

/// Wrap a check future in a hard timeout so a wedged endpoint cannot stall
/// the entire smoke run.
async fn with_timeout<F>(name: &'static str, fut: F) -> CheckResult
where
    F: std::future::Future<Output = Result<(u128, String)>>,
{
    let start = Instant::now();
    match tokio::time::timeout(PER_CHECK_TIMEOUT, fut).await {
        Ok(Ok((ms, note))) => CheckResult::pass(name, ms, note),
        Ok(Err(e)) => CheckResult::fail(name, start.elapsed().as_millis(), format_err(&e)),
        Err(_) => CheckResult::fail(
            name,
            PER_CHECK_TIMEOUT.as_millis(),
            "timed out after 30s waiting for response",
        ),
    }
}

/// Variant of `with_timeout` that returns a captured value (used to thread the
/// assistant tool-call message from check 5 into check 6).
async fn with_timeout_and_value<F, T>(name: &'static str, fut: F) -> (CheckResult, Option<T>)
where
    F: std::future::Future<Output = Result<(u128, String, Option<T>)>>,
{
    let start = Instant::now();
    match tokio::time::timeout(PER_CHECK_TIMEOUT, fut).await {
        Ok(Ok((ms, note, val))) => (CheckResult::pass(name, ms, note), val),
        Ok(Err(e)) => (
            CheckResult::fail(name, start.elapsed().as_millis(), format_err(&e)),
            None,
        ),
        Err(_) => (
            CheckResult::fail(
                name,
                PER_CHECK_TIMEOUT.as_millis(),
                "timed out after 30s waiting for response",
            ),
            None,
        ),
    }
}

/// Render an error as a single line, trimming and bounding length so the
/// per-check output stays readable.
fn format_err(e: &anyhow::Error) -> String {
    let s = format!("{:#}", e).replace('\n', " | ");
    if s.len() > 400 {
        format!("{}…", &s[..400])
    } else {
        s
    }
}

// ── Check 1: reachability ────────────────────────────────────────────────────

async fn check_reachable(
    http: &HttpClient,
    endpoint: &str,
    api_key: Option<&str>,
) -> (
    Option<Value>,
    Option<reqwest::header::HeaderMap>,
    CheckResult,
) {
    let url = format!("{}/models", endpoint.trim_end_matches('/'));
    let start = Instant::now();
    let mut req = http.get(&url);
    if let Some(key) = api_key {
        req = req.bearer_auth(key);
    }
    let result = req.send().await;
    let elapsed = start.elapsed().as_millis();

    match result {
        Ok(resp) => {
            let status = resp.status();
            let headers = resp.headers().clone();
            let text = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                return (
                    None,
                    Some(headers),
                    CheckResult::fail(
                        "endpoint_reachable",
                        elapsed,
                        format!(
                            "GET /v1/models returned HTTP {} — body: {}",
                            status.as_u16(),
                            truncate(&text, 200)
                        ),
                    ),
                );
            }
            match serde_json::from_str::<Value>(&text) {
                Ok(body) => {
                    let count = body
                        .get("data")
                        .and_then(|d| d.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    if count == 0 {
                        (
                            Some(body),
                            Some(headers),
                            CheckResult::fail(
                                "endpoint_reachable",
                                elapsed,
                                format!("HTTP 200 but `data` is empty: {}", truncate(&text, 200)),
                            ),
                        )
                    } else {
                        (
                            Some(body),
                            Some(headers),
                            CheckResult::pass(
                                "endpoint_reachable",
                                elapsed,
                                format!("{} model(s) listed", count),
                            ),
                        )
                    }
                }
                Err(e) => (
                    None,
                    Some(headers),
                    CheckResult::fail(
                        "endpoint_reachable",
                        elapsed,
                        format!(
                            "HTTP 200 but body was not JSON ({}): {}",
                            e,
                            truncate(&text, 200)
                        ),
                    ),
                ),
            }
        }
        Err(e) => (
            None,
            None,
            CheckResult::fail(
                "endpoint_reachable",
                elapsed,
                format!("network error: {}", e),
            ),
        ),
    }
}

fn first_model_id(body: &Option<Value>) -> Option<String> {
    body.as_ref()?
        .get("data")?
        .as_array()?
        .iter()
        .find_map(|item| {
            item.get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

// ── Check 2: backend classification ──────────────────────────────────────────

async fn check_backend(
    http: &HttpClient,
    endpoint: &str,
    models_body: Option<&Value>,
    models_headers: Option<&reqwest::header::HeaderMap>,
) -> CheckResult {
    let start = Instant::now();
    let base = endpoint.trim_end_matches('/');

    // Inspect headers from the /v1/models response first.
    if let Some(headers) = models_headers {
        if let Some(server) = headers
            .get(reqwest::header::SERVER)
            .and_then(|v| v.to_str().ok())
        {
            let lower = server.to_lowercase();
            if lower.contains("vllm") {
                return CheckResult::pass(
                    "backend_classify",
                    start.elapsed().as_millis(),
                    format!("vllm (Server header: {})", server),
                );
            }
            if lower.contains("sglang") {
                return CheckResult::pass(
                    "backend_classify",
                    start.elapsed().as_millis(),
                    format!("sglang (Server header: {})", server),
                );
            }
        }
    }

    // SGLang exposes /get_server_info on the same host (without /v1 prefix).
    let host_root = strip_v1(base);
    if let Ok(resp) = http
        .get(format!("{}/get_server_info", host_root))
        .timeout(Duration::from_secs(5))
        .send()
        .await
    {
        if resp.status().is_success() {
            return CheckResult::pass(
                "backend_classify",
                start.elapsed().as_millis(),
                "sglang (/get_server_info responded)",
            );
        }
    }

    // vLLM exposes /version under the /v1 base.
    if let Ok(resp) = http
        .get(format!("{}/version", base))
        .timeout(Duration::from_secs(5))
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                if text.to_lowercase().contains("vllm") {
                    return CheckResult::pass(
                        "backend_classify",
                        start.elapsed().as_millis(),
                        format!("vllm (/version: {})", truncate(text.trim(), 80)),
                    );
                }
            }
        }
    }

    // Ollama exposes /api/tags off the host root.
    if let Ok(resp) = http
        .get(format!("{}/api/tags", host_root))
        .timeout(Duration::from_secs(5))
        .send()
        .await
    {
        if resp.status().is_success() {
            return CheckResult::pass(
                "backend_classify",
                start.elapsed().as_millis(),
                "ollama (/api/tags responded)",
            );
        }
    }

    // Heuristics from the /v1/models JSON body.
    if let Some(body) = models_body {
        if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
            for item in data {
                let raw = item.to_string().to_lowercase();
                if raw.contains("lm-studio") || raw.contains("lmstudio") {
                    return CheckResult::pass(
                        "backend_classify",
                        start.elapsed().as_millis(),
                        "lm-studio (model id contains 'lm-studio')",
                    );
                }
                if let Some(owned_by) = item.get("owned_by").and_then(|v| v.as_str()) {
                    let lower = owned_by.to_lowercase();
                    if lower.contains("vllm") {
                        return CheckResult::pass(
                            "backend_classify",
                            start.elapsed().as_millis(),
                            format!("vllm (owned_by={})", owned_by),
                        );
                    }
                    if lower.contains("sglang") {
                        return CheckResult::pass(
                            "backend_classify",
                            start.elapsed().as_millis(),
                            format!("sglang (owned_by={})", owned_by),
                        );
                    }
                }
            }
        }
    }

    CheckResult::pass(
        "backend_classify",
        start.elapsed().as_millis(),
        "unknown (OpenAI-compatible) — no distinguishing markers found",
    )
}

fn strip_v1(base: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    if let Some(stripped) = trimmed.strip_suffix("/v1") {
        stripped.to_string()
    } else {
        trimmed.to_string()
    }
}

// ── Shared: build a Config + ApiClient for a single check ────────────────────

/// Build a `Config` tuned for smoke-testing: aggressive timeouts, no retries,
/// generous context window, and an optional `extra_body` overlay so each
/// check can opt into backend-specific knobs (e.g. `chat_template_kwargs`).
fn build_config(
    endpoint: &str,
    model: &str,
    api_key: Option<&str>,
    native_function_calling: bool,
    extra_body: Option<serde_json::Map<String, Value>>,
) -> Config {
    let mut cfg = Config {
        endpoint: endpoint.to_string(),
        model: model.to_string(),
        api_key: api_key.map(RedactedString::new),
        max_tokens: 256,
        context_length: 32_768,
        temperature: 0.0,
        agent: AgentConfig {
            native_function_calling,
            step_timeout_secs: 30,
            ..AgentConfig::default()
        },
        extra_body,
        ..Config::default()
    };
    cfg.retry.max_retries = 0;
    cfg
}

fn build_client(cfg: &Config) -> Result<ApiClient> {
    let client = ApiClient::new(cfg)?;
    Ok(client.with_retry_config(RetryConfig {
        max_retries: 0,
        initial_delay_ms: 200,
        max_delay_ms: 1_000,
        retryable_status_codes: vec![],
    }))
}

// ── Check 3: plain chat ──────────────────────────────────────────────────────

async fn check_plain_chat(
    endpoint: &str,
    model: &str,
    api_key: Option<&str>,
) -> Result<(u128, String)> {
    let cfg = build_config(endpoint, model, api_key, false, None);
    let client = build_client(&cfg)?;
    let messages = vec![
        Message::system(
            "You are a smoke-test responder. When asked, reply with the single \
             requested word and nothing else.",
        ),
        Message::user("Reply with the single word 'pong'."),
    ];

    let start = Instant::now();
    let resp = client
        .chat(messages, None, ThinkingMode::Disabled)
        .await
        .context("plain chat request failed")?;
    let elapsed = start.elapsed().as_millis();

    let content = resp
        .choices
        .first()
        .map(|c| c.message.content.text_all())
        .unwrap_or_default();
    if !contains_ci(&content, "pong") {
        anyhow::bail!(
            "response did not contain 'pong'. content={}",
            truncate(&content, 200)
        );
    }
    Ok((
        elapsed,
        format!(
            "got 'pong' in {}-char reply (usage: {}+{}={} tokens)",
            content.len(),
            resp.usage.prompt_tokens,
            resp.usage.completion_tokens,
            resp.usage.total_tokens
        ),
    ))
}

fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

// ── Check 4: streaming ───────────────────────────────────────────────────────

async fn check_streaming(
    endpoint: &str,
    model: &str,
    api_key: Option<&str>,
) -> Result<(u128, String)> {
    let cfg = build_config(endpoint, model, api_key, false, None);
    let client = build_client(&cfg)?;
    let messages = vec![
        Message::system(
            "You are a smoke-test responder. When asked, reply with the single \
             requested word and nothing else.",
        ),
        Message::user("Reply with the single word 'pong'."),
    ];

    let start = Instant::now();
    let stream = client
        .chat_stream(messages, None, ThinkingMode::Disabled)
        .await
        .context("streaming chat request failed")?;

    let mut rx = stream.into_channel().await;
    let mut content_chunks = 0usize;
    let mut total_chunks = 0usize;
    let mut accumulated = String::new();
    let mut got_done = false;
    while let Some(item) = rx.recv().await {
        let chunk = item.context("error reading stream chunk")?;
        total_chunks += 1;
        match chunk {
            StreamChunk::Content(text) => {
                content_chunks += 1;
                accumulated.push_str(&text);
            }
            StreamChunk::Done => {
                got_done = true;
                break;
            }
            _ => {}
        }
    }
    let elapsed = start.elapsed().as_millis();

    if total_chunks == 0 {
        anyhow::bail!("no chunks received from stream");
    }
    if !contains_ci(&accumulated, "pong") {
        anyhow::bail!(
            "streamed text missing 'pong' (chunks={}, accumulated={})",
            total_chunks,
            truncate(&accumulated, 200)
        );
    }
    Ok((
        elapsed,
        format!(
            "{} chunk(s) total, {} content chunk(s), final='{}'{}",
            total_chunks,
            content_chunks,
            truncate(accumulated.trim(), 60),
            if got_done { "" } else { " (no [DONE])" },
        ),
    ))
}

// ── Check 5: tool call ───────────────────────────────────────────────────────

fn calculator_tool() -> ToolDefinition {
    ToolDefinition {
        def_type: "function".to_string(),
        function: FunctionDefinition {
            name: "calculator".to_string(),
            description: "Multiply two integers and return the product.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "a": {
                        "type": "integer",
                        "description": "First integer"
                    },
                    "b": {
                        "type": "integer",
                        "description": "Second integer"
                    }
                },
                "required": ["a", "b"]
            }),
        },
    }
}

async fn check_tool_call(
    endpoint: &str,
    model: &str,
    api_key: Option<&str>,
) -> Result<(u128, String, Option<Message>)> {
    let cfg = build_config(endpoint, model, api_key, true, None);
    let client = build_client(&cfg)?;
    let tools = vec![calculator_tool()];
    let messages = vec![
        Message::system(
            "You have access to a `calculator` tool that multiplies two integers. \
             When the user asks for an arithmetic result, you MUST call the tool — \
             do not compute it yourself.",
        ),
        Message::user("What is 17 * 23? Use the calculator tool."),
    ];

    let start = Instant::now();
    let resp = client
        .chat(messages, Some(tools), ThinkingMode::Disabled)
        .await
        .context("tool-call chat request failed")?;
    let elapsed = start.elapsed().as_millis();

    let choice = resp
        .choices
        .first()
        .ok_or_else(|| anyhow::anyhow!("response had no choices"))?;
    let message = &choice.message;

    let tool_calls = match &message.tool_calls {
        Some(calls) if !calls.is_empty() => calls,
        _ => anyhow::bail!(
            "model did not return any tool_calls (finish_reason={:?}, content={})",
            choice.finish_reason,
            truncate(&message.content.text_all(), 200)
        ),
    };

    let call = &tool_calls[0];
    if call.function.name != "calculator" {
        anyhow::bail!(
            "model called the wrong tool: '{}' (expected 'calculator')",
            call.function.name
        );
    }
    let args: Value = serde_json::from_str(&call.function.arguments).with_context(|| {
        format!(
            "tool arguments were not valid JSON: {}",
            truncate(&call.function.arguments, 200)
        )
    })?;
    let a = args.get("a").and_then(|v| v.as_i64());
    let b = args.get("b").and_then(|v| v.as_i64());
    let (a, b) = match (a, b) {
        (Some(a), Some(b)) => (a, b),
        _ => anyhow::bail!(
            "tool arguments missing integer a/b: {}",
            truncate(&call.function.arguments, 200)
        ),
    };
    let pair_ok = (a == 17 && b == 23) || (a == 23 && b == 17);
    if !pair_ok {
        anyhow::bail!(
            "tool args wrong: a={}, b={} (expected 17 and 23 in either order)",
            a,
            b
        );
    }
    Ok((
        elapsed,
        format!("model called calculator(a={}, b={}) [id={}]", a, b, call.id),
        Some(message.clone()),
    ))
}

// ── Check 6: multi-turn tool execution ───────────────────────────────────────

async fn check_tool_followup(
    endpoint: &str,
    model: &str,
    api_key: Option<&str>,
    assistant_msg: Message,
) -> Result<(u128, String)> {
    let cfg = build_config(endpoint, model, api_key, true, None);
    let client = build_client(&cfg)?;
    let tools = vec![calculator_tool()];

    let tool_call_id = assistant_msg
        .tool_calls
        .as_ref()
        .and_then(|calls| calls.first())
        .map(|c| c.id.clone())
        .ok_or_else(|| anyhow::anyhow!("prior assistant message had no tool_call id"))?;

    let messages = vec![
        Message::system(
            "You have access to a `calculator` tool that multiplies two integers. \
             When the user asks for an arithmetic result, you MUST call the tool — \
             do not compute it yourself.",
        ),
        Message::user("What is 17 * 23? Use the calculator tool."),
        assistant_msg,
        Message::tool("391", tool_call_id),
    ];

    let start = Instant::now();
    let resp = client
        .chat(messages, Some(tools), ThinkingMode::Disabled)
        .await
        .context("tool-followup chat request failed")?;
    let elapsed = start.elapsed().as_millis();

    let content = resp
        .choices
        .first()
        .map(|c| c.message.content.text_all())
        .unwrap_or_default();
    if !content.contains("391") {
        anyhow::bail!(
            "follow-up reply did not echo 391. content={}",
            truncate(&content, 200)
        );
    }
    Ok((
        elapsed,
        format!(
            "model emitted 391 (reply: '{}')",
            truncate(content.trim(), 60)
        ),
    ))
}

// ── Check 7: thinking parsing ────────────────────────────────────────────────

async fn check_thinking(
    endpoint: &str,
    model: &str,
    api_key: Option<&str>,
) -> Result<(u128, String)> {
    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        json!({ "enable_thinking": true }),
    );
    let cfg = build_config(endpoint, model, api_key, false, Some(extra));
    let client = build_client(&cfg)?;

    // A reasoning prompt that should trigger a thinking block on capable models.
    let messages = vec![
        Message::system(
            "Solve the problem. Show your reasoning in the reasoning channel \
             only — your final answer must be a single number.",
        ),
        Message::user(
            "Alice has 12 apples. She gives half to Bob, then eats two of \
             the remainder. How many apples does Alice have left? Answer with \
             a number only.",
        ),
    ];

    let start = Instant::now();
    let resp = client
        .chat(messages, None, ThinkingMode::Enabled)
        .await
        .context("thinking-mode chat request failed")?;
    let elapsed = start.elapsed().as_millis();

    let choice = resp
        .choices
        .first()
        .ok_or_else(|| anyhow::anyhow!("response had no choices"))?;
    let content = choice.message.content.text_all();
    let reasoning = choice
        .reasoning_content
        .clone()
        .or_else(|| choice.message.reasoning_content.clone())
        .unwrap_or_default();

    // Detect leakage: a `<think>` block inside `content` means the backend did
    // not split reasoning into the dedicated channel.
    let leaked = content.contains("<think>") || content.contains("</think>");

    if reasoning.is_empty() {
        // Model may not support thinking — that's a soft pass with a note,
        // since not every endpoint will. We still flag content leakage as a
        // hard fail because that breaks selfware's downstream parsing.
        if leaked {
            anyhow::bail!(
                "no reasoning_content but `<think>` tags leaked into content: {}",
                truncate(&content, 200)
            );
        }
        return Ok((
            elapsed,
            format!(
                "no reasoning_content (model likely doesn't support thinking) — \
                 content was clean ('{}')",
                truncate(content.trim(), 60)
            ),
        ));
    }

    if leaked {
        anyhow::bail!(
            "reasoning_content present ({} chars) but `<think>` tags also leaked into content",
            reasoning.len()
        );
    }
    Ok((
        elapsed,
        format!(
            "reasoning_content={} chars, content='{}' (clean)",
            reasoning.len(),
            truncate(content.trim(), 60)
        ),
    ))
}

// ── Check 8: multimodal ──────────────────────────────────────────────────────

async fn check_multimodal(
    endpoint: &str,
    model: &str,
    api_key: Option<&str>,
    image_path: &str,
) -> Result<(u128, String)> {
    let bytes = std::fs::read(image_path)
        .with_context(|| format!("failed to read image {}", image_path))?;
    let mime = sniff_image_mime(&bytes).ok_or_else(|| {
        anyhow::anyhow!(
            "could not determine image mime type for {} (only PNG/JPEG/GIF/WebP supported)",
            image_path
        )
    })?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let data_url = format!("data:{};base64,{}", mime, b64);

    let content = MessageContent::Blocks(vec![
        ContentBlock::Text {
            text: "Describe this image in one short sentence.".to_string(),
        },
        ContentBlock::ImageUrl {
            image_url: ImageUrl {
                url: data_url,
                detail: None,
            },
        },
    ]);
    let messages = vec![Message::user_multimodal(content)];

    let cfg = build_config(endpoint, model, api_key, false, None);
    let client = build_client(&cfg)?;

    let start = Instant::now();
    let resp = client
        .chat(messages, None, ThinkingMode::Disabled)
        .await
        .context("multimodal chat request failed")?;
    let elapsed = start.elapsed().as_millis();

    let text = resp
        .choices
        .first()
        .map(|c| c.message.content.text_all())
        .unwrap_or_default();
    if text.trim().is_empty() {
        anyhow::bail!("multimodal response was empty");
    }
    Ok((
        elapsed,
        format!(
            "{}-byte image, model replied: '{}'",
            bytes.len(),
            truncate(text.trim(), 80)
        ),
    ))
}

fn sniff_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() < 4 {
        return None;
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF8") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    None
}
