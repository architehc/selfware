//! LLM Doctor — diagnostic module for local LLM backend configuration.
//!
//! Detects the backend type (sglang, vllm, ollama, llama.cpp, lmstudio),
//! analyses model capabilities, checks context length and chat templates,
//! runs a connectivity/latency test, probes streaming + tool calling +
//! `chat_template_kwargs` (Qwen-style thinking) support, and prints a
//! unified `[PASS]/[WARN]/[FAIL]` capabilities matrix with remediation
//! hints on failure.

use anyhow::Result;
use colored::Colorize;
use reqwest::Client;
use serde_json::Value;
use std::time::{Duration, Instant};

use crate::api::merge_extra_body;
use crate::config::Config;
use crate::doctor::{config_checks, CheckStatus as DoctorCheckStatus};
use crate::testing::verification::truncate_str;

// ── Timeout applied to every HTTP probe ──────────────────────────────────────
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const MIN_CONNECTION_TEST_TIMEOUT_SECS: u64 = 30;
const MAX_CONNECTION_TEST_TIMEOUT_SECS: u64 = 45;

// ── Minimum recommended context length ───────────────────────────────────────
const MIN_RECOMMENDED_CONTEXT: u64 = 32_768;

// ── Backend detection ────────────────────────────────────────────────────────

/// Recognised LLM backends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Backend {
    Sglang,
    Vllm,
    Ollama,
    LlamaCpp,
    LmStudio,
    Unknown(String),
}

impl std::fmt::Display for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Backend::Sglang => write!(f, "sglang"),
            Backend::Vllm => write!(f, "vllm"),
            Backend::Ollama => write!(f, "ollama"),
            Backend::LlamaCpp => write!(f, "llama.cpp"),
            Backend::LmStudio => write!(f, "lmstudio"),
            Backend::Unknown(hint) => write!(f, "unknown ({})", hint),
        }
    }
}

/// Model information extracted from the backend.
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub max_model_len: Option<u64>,
    pub raw: Value,
}

/// Results from detecting the backend.
#[derive(Debug)]
struct DetectionResult {
    backend: Backend,
    models: Vec<ModelInfo>,
    endpoint: String,
}

/// Results from the connection test.
#[derive(Debug)]
struct ConnectionTestResult {
    latency: Duration,
    tokens_per_second: Option<f64>,
    tool_calling_works: Option<bool>,
}

// ── Structured report ────────────────────────────────────────────────────────

/// Status of an individual doctor check.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum CheckOutcome {
    /// Check passed.
    Pass,
    /// Check produced a warning.
    Warn,
    /// Check failed.
    Fail,
    /// Check was skipped (e.g. endpoint unreachable).
    Skip,
}

impl From<DoctorCheckStatus> for CheckOutcome {
    fn from(s: DoctorCheckStatus) -> Self {
        match s {
            DoctorCheckStatus::Ok => CheckOutcome::Pass,
            DoctorCheckStatus::Warning => CheckOutcome::Warn,
            DoctorCheckStatus::Missing => CheckOutcome::Fail,
        }
    }
}

/// A single check result within the structured doctor report.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DoctorCheckResult {
    /// Human-readable name of the check.
    pub name: String,
    /// Outcome (pass / warn / fail / skip).
    pub status: CheckOutcome,
    /// Detail message.
    pub detail: String,
    /// Optional remediation hint.
    pub fix_hint: Option<String>,
}

/// Structured report returned by [`run_llm_doctor`].
///
/// Contains the outcome of every probe so callers (e.g. JSON mode, CI
/// scripts, programmatic consumers) can inspect the results without parsing
/// human-readable terminal output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DoctorReport {
    /// Config sanity checks (Step 0).
    pub config_checks: Vec<DoctorCheckResult>,
    /// Endpoint reachability check (Step 1).
    pub endpoint_reachable: Option<DoctorCheckResult>,
    /// Whether the configured model is available on the endpoint.
    pub model_available: Option<DoctorCheckResult>,
    /// Detected backend name (e.g. "vllm", "sglang", "ollama").
    pub backend: Option<String>,
    /// Model IDs listed by the endpoint.
    pub available_models: Vec<String>,
    /// Connection latency in milliseconds (Step 5).
    pub latency_ms: Option<u64>,
    /// Estimated throughput in tokens/second (Step 5).
    pub tokens_per_second: Option<f64>,
    /// Whether tool calling produced tool_calls (Step 5).
    pub tool_calling_works: Option<bool>,
    /// Capability matrix (Step 7).
    pub capabilities: Vec<DoctorCheckResult>,
    /// `true` if any check had a FAIL outcome.
    pub had_failures: bool,
}

impl DoctorReport {
    /// Create an empty report.
    fn new() -> Self {
        Self {
            config_checks: Vec::new(),
            endpoint_reachable: None,
            model_available: None,
            backend: None,
            available_models: Vec::new(),
            latency_ms: None,
            tokens_per_second: None,
            tool_calling_works: None,
            capabilities: Vec::new(),
            had_failures: false,
        }
    }
}

// ── Public entry-point ───────────────────────────────────────────────────────

/// Run the full LLM doctor diagnostic.
///
/// Prints human-readable output to stdout for interactive use **and** returns
/// a structured [`DoctorReport`] so callers (JSON mode, CI, programmatic
/// consumers) can inspect individual check outcomes without parsing terminal
/// text.
///
/// Returns `Ok(report)` on success and `Err(...)` if any FAIL-level check is
/// encountered (so the binary can exit with a non-zero code in CI / scripts).
/// The `DoctorReport` is still attached to the error via [`anyhow::Context`]
/// when possible, but callers that want the report even on failure can use
/// [`run_llm_doctor_report`] instead.
pub async fn run_llm_doctor(config: &Config) -> Result<DoctorReport> {
    let (report, had_fail) = run_llm_doctor_inner(config).await?;
    if had_fail {
        eprintln!(
            "{}",
            "  llm-doctor finished with FAILures — see remediation hints above."
                .red()
                .bold()
        );
        return Err(anyhow::anyhow!("llm-doctor: one or more checks failed"));
    }
    Ok(report)
}

/// Run the full LLM doctor diagnostic and always return the structured
/// report, even if some checks failed.  The `had_failures` field on the
/// report indicates whether any FAIL-level check was encountered.
pub async fn run_llm_doctor_report(config: &Config) -> Result<DoctorReport> {
    let (report, _) = run_llm_doctor_inner(config).await?;
    Ok(report)
}

/// Internal implementation shared by both public entry points.
async fn run_llm_doctor_inner(config: &Config) -> Result<(DoctorReport, bool)> {
    let mut report = DoctorReport::new();
    let mut had_fail = false;
    println!();
    println!(
        "{}",
        "╭─────────────────────────────────────────────╮"
            .bold()
            .cyan()
    );
    println!(
        "{}",
        "│         LLM Doctor — Backend Diagnostic     │"
            .bold()
            .cyan()
    );
    println!(
        "{}",
        "╰─────────────────────────────────────────────╯"
            .bold()
            .cyan()
    );
    println!();

    let endpoint = config.endpoint.clone();
    let model_name = config.model.clone();

    // ── Step 0: Config sanity checks (unified [PASS]/[WARN]/[FAIL] format) ──
    println!("{}", "Step 0: Config Sanity".bold().underline());
    for c in config_checks(config) {
        let printed_fail =
            print_unified_check(&c.name, c.status, &c.message, c.fix_hint.as_deref());
        had_fail |= printed_fail;
        report.config_checks.push(DoctorCheckResult {
            name: c.name,
            status: c.status.into(),
            detail: c.message,
            fix_hint: c.fix_hint,
        });
    }
    println!();

    // Step 1: Detect Backend
    println!("{}", "Step 1: Detecting Backend".bold().underline());
    let detection = detect_backend(&endpoint).await;

    match &detection {
        Ok(det) => {
            print_unified_check(
                "endpoint reachable",
                DoctorCheckStatus::Ok,
                &format!("{} (backend: {})", det.endpoint, det.backend),
                None,
            );
            report.endpoint_reachable = Some(DoctorCheckResult {
                name: "endpoint reachable".to_string(),
                status: CheckOutcome::Pass,
                detail: format!("{} (backend: {})", det.endpoint, det.backend),
                fix_hint: None,
            });
            report.backend = Some(det.backend.to_string());
            report.available_models = det.models.iter().map(|m| m.id.clone()).collect();
            println!(
                "  {} Models available: {}",
                ">>".green(),
                det.models.len().to_string().bright_white()
            );
            for m in &det.models {
                let ctx = m
                    .max_model_len
                    .map(|l| format!(" (ctx: {})", l))
                    .unwrap_or_default();
                println!("     - {}{}", m.id.bright_white(), ctx.dimmed());
            }
            // Configured model is in the list?
            let configured_in_list = det.models.iter().any(|m| {
                m.id == model_name || m.id.contains(&model_name) || model_name.contains(&m.id)
            });
            if configured_in_list {
                print_unified_check(
                    "configured model available",
                    DoctorCheckStatus::Ok,
                    &model_name,
                    None,
                );
                report.model_available = Some(DoctorCheckResult {
                    name: "configured model available".to_string(),
                    status: CheckOutcome::Pass,
                    detail: model_name.clone(),
                    fix_hint: None,
                });
            } else {
                had_fail |= print_unified_check(
                    "configured model available",
                    DoctorCheckStatus::Missing,
                    &format!("`{}` is not listed by the endpoint", model_name),
                    Some(
                        "Set selfware.toml `model` to one of the available IDs above, or load that model in your backend.",
                    ),
                );
                report.model_available = Some(DoctorCheckResult {
                    name: "configured model available".to_string(),
                    status: CheckOutcome::Fail,
                    detail: format!("`{}` is not listed by the endpoint", model_name),
                    fix_hint: Some("Set selfware.toml `model` to one of the available IDs above, or load that model in your backend.".to_string()),
                });
            }
        }
        Err(e) => {
            print_unified_check(
                "endpoint reachable",
                DoctorCheckStatus::Missing,
                &format!("could not reach `{}`: {}", endpoint, e),
                Some("Start your local LLM backend (vLLM / SGLang / Ollama / LM Studio) and verify the endpoint URL in selfware.toml."),
            );
            report.endpoint_reachable = Some(DoctorCheckResult {
                name: "endpoint reachable".to_string(),
                status: CheckOutcome::Fail,
                detail: format!("could not reach `{}`: {}", endpoint, e),
                fix_hint: Some("Start your local LLM backend (vLLM / SGLang / Ollama / LM Studio) and verify the endpoint URL in selfware.toml.".to_string()),
            });
            report.had_failures = true;
            println!();
            // No point continuing — return a hard error so the caller
            // (cli::run) can exit with a non-zero code gracefully.
            return Err(anyhow::anyhow!(
                "LLM endpoint `{}` is unreachable: {}",
                endpoint,
                e
            ));
        }
    }
    println!();

    let det = detection.unwrap();

    // Step 2: Model Analysis
    println!("{}", "Step 2: Model Analysis".bold().underline());
    analyse_model(&det, config);
    println!();

    // Step 3: Template / Chat Format Check
    println!(
        "{}",
        "Step 3: Template / Chat Format Check".bold().underline()
    );
    check_template(&det, config);
    println!();

    // Step 4: Capability Assessment
    println!("{}", "Step 4: Capability Assessment".bold().underline());
    assess_capabilities(&model_name);
    println!();

    // Step 5: Connection Test
    println!("{}", "Step 5: Connection Test".bold().underline());
    let conn_result = connection_test(&endpoint, &model_name, config).await;
    match &conn_result {
        Ok(res) => {
            report.latency_ms = Some(res.latency.as_millis() as u64);
            report.tokens_per_second = res.tokens_per_second;
            report.tool_calling_works = res.tool_calling_works;
            println!(
                "  {} Response latency: {:.0}ms",
                ">>".green(),
                res.latency.as_millis()
            );
            if let Some(tps) = res.tokens_per_second {
                println!(
                    "  {} Estimated throughput: {:.1} tokens/s",
                    ">>".green(),
                    tps
                );
            }
            match res.tool_calling_works {
                Some(true) => {
                    println!(
                        "  {} Tool calling: {}",
                        ">>".green(),
                        "working".green().bold()
                    );
                }
                Some(false) => {
                    println!(
                        "  {} Tool calling: {}",
                        "!!".yellow(),
                        "not working or unsupported".yellow()
                    );
                }
                None => {
                    println!(
                        "  {} Tool calling: {}",
                        "--".dimmed(),
                        "skipped (could not test)".dimmed()
                    );
                }
            }
        }
        Err(e) => {
            println!(
                "  {} Connection test failed: {}",
                "!!".red().bold(),
                e.to_string().red()
            );
        }
    }
    println!();

    // Step 6: Recommendations Tree
    println!("{}", "Step 6: Recommendations".bold().underline());
    print_recommendations(&det, config, conn_result.as_ref().ok());
    println!();

    // Step 7: Capabilities matrix (tools / streaming / thinking / multimodal)
    println!("{}", "Step 7: Capabilities Matrix".bold().underline());
    let caps = probe_capabilities(&endpoint, &model_name, config).await;
    let tools_status = match (
        conn_result.as_ref().ok().and_then(|r| r.tool_calling_works),
        caps.tools,
    ) {
        (Some(true), _) | (_, Some(true)) => DoctorCheckStatus::Ok,
        (Some(false), _) | (_, Some(false)) => DoctorCheckStatus::Warning,
        _ => DoctorCheckStatus::Warning,
    };
    let tools_detail = match tools_status {
        DoctorCheckStatus::Ok => "model honored a calculator tool call",
        DoctorCheckStatus::Warning => "model did not produce tool_calls",
        DoctorCheckStatus::Missing => "tool calling probe failed",
    };
    let tools_fix = if tools_status == DoctorCheckStatus::Ok {
        None
    } else {
        Some("vLLM: pass `--enable-auto-tool-choice --tool-call-parser hermes`. SGLang: ensure a tool-aware chat template. Ollama: upgrade to >= 0.5.0.")
    };
    had_fail |= print_unified_check("tool calling", tools_status, tools_detail, tools_fix);
    report.capabilities.push(DoctorCheckResult {
        name: "tool calling".to_string(),
        status: tools_status.into(),
        detail: tools_detail.to_string(),
        fix_hint: tools_fix.map(String::from),
    });

    let streaming_ok = caps.streaming.unwrap_or(false);
    let streaming_status = if streaming_ok {
        DoctorCheckStatus::Ok
    } else {
        DoctorCheckStatus::Warning
    };
    let streaming_detail = if streaming_ok {
        "SSE streaming responded with chunks"
    } else {
        "streaming not available or returned no chunks"
    };
    let streaming_fix = if streaming_ok {
        None
    } else {
        Some("Ensure the backend supports SSE streaming (default for vLLM/SGLang/Ollama). Disable any reverse proxy buffering.")
    };
    had_fail |= print_unified_check(
        "streaming",
        streaming_status,
        streaming_detail,
        streaming_fix,
    );
    report.capabilities.push(DoctorCheckResult {
        name: "streaming".to_string(),
        status: streaming_status.into(),
        detail: streaming_detail.to_string(),
        fix_hint: streaming_fix.map(String::from),
    });

    let thinking_ok = caps.thinking.unwrap_or(false);
    let thinking_status = if thinking_ok {
        DoctorCheckStatus::Ok
    } else {
        DoctorCheckStatus::Warning
    };
    let thinking_detail = if thinking_ok {
        "backend accepts chat_template_kwargs (Qwen-style thinking)"
    } else {
        "chat_template_kwargs not supported (or backend ignored it)"
    };
    let thinking_fix = if thinking_ok {
        None
    } else {
        Some("Required for Qwen3.5 thinking control. Use vLLM/SGLang with a Qwen template, or set `[extra_body] chat_template_kwargs = { enable_thinking = false }` to opt out.")
    };
    had_fail |= print_unified_check(
        "chat_template_kwargs (thinking)",
        thinking_status,
        thinking_detail,
        thinking_fix,
    );
    report.capabilities.push(DoctorCheckResult {
        name: "chat_template_kwargs (thinking)".to_string(),
        status: thinking_status.into(),
        detail: thinking_detail.to_string(),
        fix_hint: thinking_fix.map(String::from),
    });

    let mm_ok = caps.multimodal;
    let mm_status = if mm_ok {
        DoctorCheckStatus::Ok
    } else {
        DoctorCheckStatus::Warning
    };
    let mm_detail = if mm_ok {
        "model name suggests vision-language support"
    } else {
        "no vision modality detected for this model"
    };
    let mm_fix = if mm_ok {
        None
    } else {
        Some("Multimodal is optional. To enable, configure a vision-language model (Qwen3.5-VL, Llava, etc.) and set `modalities = [\"text\", \"vision\"]`.")
    };
    had_fail |= print_unified_check("multimodal (vision)", mm_status, mm_detail, mm_fix);
    report.capabilities.push(DoctorCheckResult {
        name: "multimodal (vision)".to_string(),
        status: mm_status.into(),
        detail: mm_detail.to_string(),
        fix_hint: mm_fix.map(String::from),
    });
    println!();

    report.had_failures = had_fail;

    Ok((report, had_fail))
}

// ── Unified output + capability probe helpers ────────────────────────────────

/// Print a single `[PASS]/[WARN]/[FAIL]` line with optional `How to fix:` hint.
/// Returns `true` if the printed status was a FAIL (so the caller can OR into a
/// `had_fail` flag).
fn print_unified_check(
    name: &str,
    status: DoctorCheckStatus,
    detail: &str,
    fix_hint: Option<&str>,
) -> bool {
    let (tag, line) = match status {
        DoctorCheckStatus::Ok => (
            "[PASS]".green().bold().to_string(),
            format!("{} — {}", name, detail).green().to_string(),
        ),
        DoctorCheckStatus::Warning => (
            "[WARN]".yellow().bold().to_string(),
            format!("{} — {}", name, detail).yellow().to_string(),
        ),
        DoctorCheckStatus::Missing => (
            "[FAIL]".red().bold().to_string(),
            format!("{} — {}", name, detail).red().to_string(),
        ),
    };
    println!("  {} {}", tag, line);
    if status != DoctorCheckStatus::Ok {
        if let Some(hint) = fix_hint {
            println!("         {} {}", "How to fix:".bold().cyan(), hint);
        }
    }
    status == DoctorCheckStatus::Missing
}

/// Capability probe results.
#[derive(Debug, Default, Clone)]
pub(crate) struct Capabilities {
    /// Tool calling worked when probed (None = not probed).
    pub tools: Option<bool>,
    /// Streaming returned at least one SSE chunk (None = not probed).
    pub streaming: Option<bool>,
    /// `chat_template_kwargs` accepted by the backend without error.
    pub thinking: Option<bool>,
    /// Configured model looks multimodal (heuristic by name).
    pub multimodal: bool,
}

async fn probe_capabilities(endpoint: &str, model: &str, config: &Config) -> Capabilities {
    let mut caps = Capabilities {
        multimodal: looks_multimodal(model),
        ..Default::default()
    };

    let probe_timeout = connection_test_timeout(config);
    let client = match Client::builder().timeout(probe_timeout).build() {
        Ok(c) => c,
        Err(_) => return caps,
    };

    let base = endpoint.trim_end_matches('/');
    let url = format!("{}/chat/completions", base);
    let api_key = config.api_key.as_ref().map(|k| k.expose().to_string());

    // ── streaming probe ──
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "Reply with the single word: hi"}],
        "max_tokens": 8,
        "stream": true,
        "temperature": 0.0,
    });
    let mut req = client.post(&url).json(&body);
    if let Some(ref k) = api_key {
        // Don't leak the key over plaintext HTTP to a remote host / userinfo URL.
        if crate::config::api_key::assert_credential_endpoint_safe(&url, true).is_ok() {
            req = req.bearer_auth(k);
        } else {
            eprintln!("  ⚠ not sending API key to unsafe endpoint {url}");
        }
    }
    caps.streaming = match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            let text = resp.text().await.unwrap_or_default();
            // Look for SSE chunk markers.
            Some(text.contains("data:") && text.contains("\n"))
        }
        Ok(_) => Some(false),
        Err(_) => Some(false),
    };

    // ── chat_template_kwargs (thinking) probe ──
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "Reply with the single word: ok"}],
        "max_tokens": 8,
        "temperature": 0.0,
        "chat_template_kwargs": { "enable_thinking": false }
    });
    let mut req = client.post(&url).json(&body);
    if let Some(ref k) = api_key {
        // Don't leak the key over plaintext HTTP to a remote host / userinfo URL.
        if crate::config::api_key::assert_credential_endpoint_safe(&url, true).is_ok() {
            req = req.bearer_auth(k);
        } else {
            eprintln!("  ⚠ not sending API key to unsafe endpoint {url}");
        }
    }
    caps.thinking = match req.send().await {
        Ok(resp) => Some(resp.status().is_success()),
        Err(_) => Some(false),
    };

    caps
}

/// Heuristic: does the model name suggest vision-language capabilities?
fn looks_multimodal(model: &str) -> bool {
    let l = model.to_lowercase();
    l.contains("vl")
        || l.contains("vision")
        || l.contains("llava")
        || l.contains("multimodal")
        || l.contains("kimi")
        || l.contains("gemini")
        || l.contains("gpt-4o")
        || l.contains("claude-3")
        || l.contains("qvq")
        || l.contains("122b") // selfware-hosted Qwen3.5-VL family commonly uses this size tag
}

// ── Step 1 implementation ────────────────────────────────────────────────────

async fn detect_backend(endpoint: &str) -> Result<DetectionResult> {
    let client = Client::builder().timeout(HTTP_TIMEOUT).build()?;

    // Strip trailing /v1 for base URL probes
    let base = endpoint.trim_end_matches('/');
    let base_no_v1 = base.trim_end_matches("/v1");

    // Try /v1/models (OpenAI-compatible)
    let models_url = format!("{}/models", base);
    let resp = client
        .get(&models_url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to {}: {}", models_url, e))?;

    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("Endpoint returned HTTP {} for GET {}", status, models_url);
    }

    // Snapshot interesting response headers BEFORE consuming the body. Backends
    // like vLLM, SGLang, and LM Studio ship distinctive headers we can use for
    // identification even if the JSON body is generic OpenAI-compatible.
    let mut header_blob = String::new();
    for (name, value) in resp.headers().iter() {
        if let Ok(v) = value.to_str() {
            header_blob.push_str(&format!("{}: {}\n", name.as_str().to_lowercase(), v));
        }
    }

    let body: Value = resp.json().await?;

    // Parse model list
    let models = parse_models(&body);

    // Header-first detection (cheap), falling back to active probes.
    let backend = match detect_backend_from_headers(&header_blob, &body) {
        Some(b) => b,
        None => identify_backend(&client, base_no_v1, &body).await,
    };

    Ok(DetectionResult {
        backend,
        models,
        endpoint: endpoint.to_string(),
    })
}

/// Try to identify the backend purely from response headers / body contents,
/// without making additional probe requests. Returns `None` if no signal is
/// found — the caller will fall back to active probing.
fn detect_backend_from_headers(header_blob: &str, models_body: &Value) -> Option<Backend> {
    let lower = header_blob.to_lowercase();
    if lower.contains("x-vllm-version") || lower.contains("server: vllm") {
        return Some(Backend::Vllm);
    }
    if lower.contains("x-sglang-version") || lower.contains("server: sglang") {
        return Some(Backend::Sglang);
    }
    if lower.contains("server: ollama") {
        return Some(Backend::Ollama);
    }
    if lower.contains("server: llama.cpp") || lower.contains("server: llama-cpp") {
        return Some(Backend::LlamaCpp);
    }
    if lower.contains("server: lm-studio") || lower.contains("lm-studio") {
        return Some(Backend::LmStudio);
    }

    // Inspect served-model-name field (vLLM) or "lm-studio" model IDs.
    if let Some(data) = models_body.get("data").and_then(|d| d.as_array()) {
        for item in data {
            let raw = item.to_string().to_lowercase();
            if raw.contains("served-model-name") || raw.contains("served_model_name") {
                // SGLang/vLLM both use this; prefer not to disambiguate here.
                if raw.contains("sglang") {
                    return Some(Backend::Sglang);
                }
                if raw.contains("vllm") {
                    return Some(Backend::Vllm);
                }
            }
            if raw.contains("lm-studio") || raw.contains("lmstudio") {
                return Some(Backend::LmStudio);
            }
        }
    }

    None
}

fn parse_models(body: &Value) -> Vec<ModelInfo> {
    let mut models = Vec::new();

    if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
        for item in data {
            let id = item
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            // Try various fields for context length
            let max_model_len = item
                .get("max_model_len")
                .and_then(|v| v.as_u64())
                .or_else(|| item.get("context_length").and_then(|v| v.as_u64()))
                .or_else(|| item.get("max_tokens").and_then(|v| v.as_u64()));

            models.push(ModelInfo {
                id,
                max_model_len,
                raw: item.clone(),
            });
        }
    }

    models
}

async fn identify_backend(client: &Client, base_url: &str, models_body: &Value) -> Backend {
    // Check for sglang-specific endpoint: /get_server_info
    if let Ok(resp) = client
        .get(format!("{}/get_server_info", base_url))
        .timeout(HTTP_TIMEOUT)
        .send()
        .await
    {
        if resp.status().is_success() {
            return Backend::Sglang;
        }
    }

    // Check for vllm-specific: /version or model metadata containing "vllm"
    if let Ok(resp) = client
        .get(format!("{}/version", base_url))
        .timeout(HTTP_TIMEOUT)
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(body) = resp.text().await {
                let lower = body.to_lowercase();
                if lower.contains("vllm") {
                    return Backend::Vllm;
                }
            }
        }
    }

    // Check for Ollama: /api/tags endpoint
    if let Ok(resp) = client
        .get(format!("{}/api/tags", base_url))
        .timeout(HTTP_TIMEOUT)
        .send()
        .await
    {
        if resp.status().is_success() {
            return Backend::Ollama;
        }
    }

    // Check for LM Studio: typically has "lm-studio" or "lmstudio" in headers or model IDs
    if let Some(data) = models_body.get("data").and_then(|d| d.as_array()) {
        for item in data {
            let raw = item.to_string().to_lowercase();
            if raw.contains("lm-studio") || raw.contains("lmstudio") {
                return Backend::LmStudio;
            }
        }
    }

    // Check for llama.cpp: /health endpoint with llama.cpp specific fields
    if let Ok(resp) = client
        .get(format!("{}/health", base_url))
        .timeout(HTTP_TIMEOUT)
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(body) = resp.text().await {
                let lower = body.to_lowercase();
                if lower.contains("slots") || lower.contains("llama") {
                    return Backend::LlamaCpp;
                }
            }
        }
    }

    // Check /v1/models response for owned_by hints
    if let Some(data) = models_body.get("data").and_then(|d| d.as_array()) {
        for item in data {
            if let Some(owned_by) = item.get("owned_by").and_then(|v| v.as_str()) {
                let lower = owned_by.to_lowercase();
                if lower.contains("vllm") {
                    return Backend::Vllm;
                }
                if lower.contains("llamacpp") || lower.contains("llama.cpp") {
                    return Backend::LlamaCpp;
                }
            }
        }
    }

    Backend::Unknown("OpenAI-compatible".to_string())
}

// ── Step 2 implementation ────────────────────────────────────────────────────

fn analyse_model(det: &DetectionResult, config: &Config) {
    let configured_model = config.model.as_str();

    // Try to find the configured model in the list
    let matching = det
        .models
        .iter()
        .find(|m| m.id == configured_model || m.id.contains(configured_model));

    let is_qwen35 = is_qwen35_model(configured_model);

    if let Some(model) = matching {
        println!(
            "  {} Configured model found: {}",
            "ok".green().bold(),
            model.id.bright_white()
        );
        println!(
            "  {} Selfware config context_length: {} tokens",
            ">>".green(),
            config.context_length.to_string().bright_white()
        );

        if let Some(ctx) = model.max_model_len {
            println!(
                "  {} Context length: {} tokens",
                ">>".green(),
                ctx.to_string().bright_white()
            );

            if config.context_length < ctx as usize {
                println!(
                    "  {} Configured context_length ({}) is below backend max_model_len ({})",
                    ">>".yellow(),
                    config.context_length,
                    ctx
                );
                println!(
                    "     Raise selfware.toml {} to use the full window.",
                    "context_length".bright_white()
                );
            } else if config.context_length > ctx as usize {
                println!(
                    "  {} Configured context_length ({}) exceeds backend max_model_len ({})",
                    "!!".yellow().bold(),
                    config.context_length,
                    ctx
                );
                println!(
                    "     Lower {} or increase the backend limit to avoid runtime overflows.",
                    "context_length".bright_white()
                );
            } else {
                println!(
                    "  {} Selfware context_length matches the backend limit",
                    "ok".green().bold()
                );
            }

            if ctx < MIN_RECOMMENDED_CONTEXT {
                println!(
                    "  {} Context length {} is below recommended minimum ({})",
                    "!!".yellow().bold(),
                    ctx,
                    MIN_RECOMMENDED_CONTEXT
                );
                print_context_extension_help(&det.backend);
            } else {
                println!("  {} Context length is sufficient", "ok".green().bold());
            }
        } else {
            println!(
                "  {} Could not determine context length from model info",
                "--".dimmed()
            );
            if is_qwen35 {
                println!(
                    "  {} Qwen3.5 models support up to 131072 tokens — ensure your backend is configured accordingly",
                    ">>".yellow()
                );
                print_context_extension_help(&det.backend);
            }
        }
    } else {
        println!(
            "  {} Configured model '{}' was not found in the backend's model list",
            "!!".yellow().bold(),
            configured_model.bright_white()
        );
        println!(
            "  {} Selfware config context_length: {} tokens",
            ">>".green(),
            config.context_length.to_string().bright_white()
        );
        if !det.models.is_empty() {
            println!("     Available models:");
            for m in &det.models {
                println!("       - {}", m.id);
            }
        }
    }

    if is_qwen35 {
        println!(
            "  {} Qwen3.5 series detected — checking model-specific recommendations",
            ">>".cyan()
        );
    }
}

fn is_qwen35_model(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("qwen3.5") || lower.contains("qwen3-5")
}

fn is_qwen_model(name: &str) -> bool {
    name.to_lowercase().contains("qwen")
}

fn print_context_extension_help(backend: &Backend) {
    println!(
        "  {} To extend context length, use the appropriate flag:",
        ">>".yellow()
    );
    match backend {
        Backend::Sglang => {
            println!(
                "     sglang: {} or {}",
                "--context-length 131072".bright_white(),
                "--max-model-len 131072".bright_white()
            );
        }
        Backend::Vllm => {
            println!("     vllm: {}", "--max-model-len 131072".bright_white());
        }
        Backend::Ollama => {
            println!(
                "     ollama: set {} in your Modelfile",
                "num_ctx 131072".bright_white()
            );
        }
        Backend::LlamaCpp => {
            println!("     llama.cpp: {}", "-c 131072".bright_white());
        }
        Backend::LmStudio => {
            println!("     LM Studio: set context length in the model settings UI");
        }
        Backend::Unknown(_) => {
            println!("     Check your backend's documentation for context length flags.");
            println!("     Common options:");
            println!(
                "       sglang:    {}",
                "--context-length 131072".bright_white()
            );
            println!(
                "       vllm:      {}",
                "--max-model-len 131072".bright_white()
            );
            println!("       llama.cpp: {}", "-c 131072".bright_white());
        }
    }
}

// ── Step 3 implementation ────────────────────────────────────────────────────

fn check_template(det: &DetectionResult, config: &Config) {
    let model_name = config.model.as_str();
    let is_qwen = is_qwen_model(model_name);

    match det.backend {
        Backend::Sglang => {
            println!(
                "  {} sglang detected — checking chat template configuration",
                ">>".green()
            );
            if is_qwen {
                println!(
                    "  {} For Qwen models, ensure the Jinja template supports tool calling.",
                    ">>".cyan()
                );
                println!(
                    "     Recommended: {} (sglang auto-detects from model metadata)",
                    "--chat-template auto".bright_white()
                );
                println!("     If tool calls fail, try specifying a template explicitly:");
                println!(
                    "       {}",
                    "--chat-template /path/to/qwen_tool_call.jinja".bright_white()
                );
                match configured_enable_thinking(config) {
                    Some(false) => {
                        println!(
                            "  {} Selfware config already sets {}",
                            "ok".green().bold(),
                            "chat_template_kwargs.enable_thinking = false".bright_white()
                        );
                    }
                    Some(true) => {
                        println!(
                            "  {} Selfware config enables thinking. For Qwen tool use on sglang, disable it.",
                            "!!".yellow().bold()
                        );
                        println!(
                            "     Add {}",
                            "[extra_body]\nchat_template_kwargs = { enable_thinking = false }"
                                .bright_white()
                        );
                    }
                    None => {
                        println!(
                            "  {} Selfware config does not set {}",
                            ">>".yellow(),
                            "chat_template_kwargs.enable_thinking".bright_white()
                        );
                        println!(
                            "     Add {} for faster, more reliable tool-heavy requests.",
                            "[extra_body]\nchat_template_kwargs = { enable_thinking = false }"
                                .bright_white()
                        );
                    }
                }
            } else {
                println!(
                    "  {} Use {} to let sglang auto-detect the template",
                    ">>".cyan(),
                    "--chat-template auto".bright_white()
                );
            }
        }
        Backend::Vllm => {
            println!(
                "  {} vllm detected — checking chat template configuration",
                ">>".green()
            );
            if is_qwen {
                println!(
                    "  {} Qwen models with vllm: the bundled chat template usually",
                    ">>".cyan()
                );
                println!("     supports tool calling out of the box.");
                println!(
                    "     If issues arise, pass {} for Hermes-style tool use.",
                    "--tool-call-parser hermes".bright_white()
                );
                println!(
                    "     Or enable the auto parser: {}",
                    "--enable-auto-tool-choice".bright_white()
                );
            } else {
                println!(
                    "  {} vllm typically auto-selects the chat template from model metadata.",
                    ">>".cyan()
                );
            }
        }
        Backend::Ollama => {
            println!(
                "  {} Ollama uses built-in templates per model — no manual config needed.",
                "ok".green().bold()
            );
            if is_qwen {
                println!(
                    "  {} Ollama's Qwen templates generally support tool calling.",
                    ">>".cyan()
                );
                println!(
                    "  {} If tool calls don't work, make sure you're using a recent",
                    ">>".yellow()
                );
                println!("     Ollama version (>= 0.5.0) with native tool support.");
            }
        }
        Backend::LlamaCpp => {
            println!(
                "  {} llama.cpp: ensure you're using {} for Qwen models",
                ">>".cyan(),
                "--chat-template chatml".bright_white()
            );
            if is_qwen {
                println!(
                    "  {} Tool calling with llama.cpp may require a custom",
                    "!!".yellow().bold()
                );
                println!("     grammar or GBNF constraint. Consider sglang or vllm for");
                println!("     full tool-calling support.");
            }
        }
        Backend::LmStudio => {
            println!(
                "  {} LM Studio: template is configured in the UI per model.",
                "ok".green().bold()
            );
            if is_qwen {
                println!(
                    "  {} Ensure \"Chat Template\" is set to the model's native format.",
                    ">>".cyan()
                );
                println!(
                    "  {} Tool calling support in LM Studio depends on the model and version.",
                    ">>".yellow()
                );
            }
        }
        Backend::Unknown(_) => {
            println!(
                "  {} Unknown backend — cannot verify chat template configuration.",
                "--".dimmed()
            );
            if is_qwen {
                println!(
                    "  {} For Qwen models, ensure the backend applies a Jinja template",
                    ">>".yellow()
                );
                println!("     that supports tool calling (function-call tokens).");
            }
        }
    }
}

// ── Step 4 implementation ────────────────────────────────────────────────────

fn assess_capabilities(model_name: &str) {
    let lower = model_name.to_lowercase();

    // Detect model family and size
    let assessment = if lower.contains("qwen3.5-122b") || lower.contains("qwen3-5-122b") {
        ModelAssessment {
            quality: "Excellent",
            summary: "Excellent for code generation, tool use, and visual processing",
            strengths: vec![
                "Complex multi-step coding tasks",
                "Tool calling and function use",
                "Visual / multimodal processing (with vision endpoint)",
                "Long-context reasoning",
            ],
            limitations: vec!["Requires significant VRAM (may need quantisation or multi-GPU)"],
        }
    } else if lower.contains("qwen3-coder") || lower.contains("qwen3.5-coder") {
        ModelAssessment {
            quality: "Very Good",
            summary: "Optimized for coding tasks",
            strengths: vec![
                "Code generation and editing",
                "Code review and refactoring",
                "Test generation",
                "Tool calling for code-related tools",
            ],
            limitations: vec![
                "May be less capable on non-code reasoning tasks",
                "Visual processing depends on model variant",
            ],
        }
    } else if is_model_small(&lower) {
        ModelAssessment {
            quality: "Limited",
            summary: "May struggle with complex multi-step tasks",
            strengths: vec![
                "Simple single-step tasks",
                "Fast response times",
                "Low resource usage",
            ],
            limitations: vec![
                "Complex multi-tool workflows may fail",
                "Long code generation quality decreases",
                "Tool calling may be unreliable",
                "Context window may be limited",
            ],
        }
    } else if lower.contains("qwen") {
        ModelAssessment {
            quality: "Good",
            summary: "Qwen model — generally good for selfware tasks",
            strengths: vec![
                "Code generation and editing",
                "Tool calling support",
                "Multi-language understanding",
            ],
            limitations: vec!["Performance depends on model size and quantisation"],
        }
    } else {
        ModelAssessment {
            quality: "Unknown",
            summary: "Unknown model — capabilities not assessed",
            strengths: vec![],
            limitations: vec!["Run the connection test (Step 5) to verify basic functionality"],
        }
    };

    println!(
        "  {} Quality tier: {}",
        ">>".green(),
        assessment.quality.bright_yellow().bold()
    );
    println!("  {} {}", ">>".green(), assessment.summary);

    if !assessment.strengths.is_empty() {
        println!("  {} {}", ">>".green(), "Strengths:".bold());
        for s in &assessment.strengths {
            println!("     {} {}", "+".green(), s);
        }
    }
    if !assessment.limitations.is_empty() {
        println!("  {} {}", ">>".yellow(), "Limitations:".bold());
        for l in &assessment.limitations {
            println!("     {} {}", "-".yellow(), l);
        }
    }

    // Feature compatibility
    println!();
    println!(
        "  {} {}",
        ">>".green(),
        "Selfware feature compatibility:".bold()
    );

    let features = [
        ("Shell tool execution", true),
        ("File editing", true),
        ("Code analysis", true),
        ("Multi-step tool workflows", assessment.quality != "Limited"),
        (
            "Tool calling (function use)",
            assessment.quality != "Limited" && assessment.quality != "Unknown",
        ),
        (
            "Visual processing",
            lower.contains("122b") || lower.contains("vision") || lower.contains("vl"),
        ),
        ("Long-context tasks (>32K)", !is_model_small(&lower)),
    ];

    for (feature, supported) in &features {
        if *supported {
            println!("     {} {}", "ok".green(), feature);
        } else {
            println!("     {} {} (may be limited)", "!!".yellow(), feature);
        }
    }
}

struct ModelAssessment {
    quality: &'static str,
    summary: &'static str,
    strengths: Vec<&'static str>,
    limitations: Vec<&'static str>,
}

fn is_model_small(lower: &str) -> bool {
    // Detect small models by parameter count in name.
    // We need to be careful not to match "72b" as "2b", so we require that
    // the digit(s) forming the param count are preceded by a separator
    // (-, _, or start of the token).
    let small_sizes = ["0.5b", "1b", "1.5b", "2b", "3b", "4b", "5b", "6b", "7b"];
    for size in &small_sizes {
        // Match "-{size}", "_{size}", or the string starting with the size
        let with_dash = format!("-{}", size);
        let with_underscore = format!("_{}", size);
        // Check each pattern, ensuring it's either at the end or followed
        // by a non-digit (to avoid "-7b" matching inside "-72b")
        for pat in [&with_dash, &with_underscore] {
            if let Some(pos) = lower.find(pat.as_str()) {
                let after = pos + pat.len();
                // Accept if at end, or next char is not alphanumeric (except known suffixes like -instruct)
                if after >= lower.len() || !lower.as_bytes()[after].is_ascii_digit() {
                    return true;
                }
            }
        }
    }
    false
}

// ── Step 5 implementation ────────────────────────────────────────────────────

async fn connection_test(
    endpoint: &str,
    model: &str,
    config: &Config,
) -> Result<ConnectionTestResult> {
    let probe_timeout = connection_test_timeout(config);
    let client = Client::builder().timeout(probe_timeout).build()?;

    let base = endpoint.trim_end_matches('/');
    let completions_url = format!("{}/chat/completions", base);

    // Build the auth header if available
    let api_key = config.api_key.as_ref().map(|k| k.expose().to_string());

    // Simple completion test
    let mut request_body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": "You are a concise assistant."},
            {"role": "user", "content": "Say 'hello' and nothing else."}
        ],
        "max_tokens": 16,
        "temperature": 0.0
    });
    merge_extra_body(
        &mut request_body,
        config.extra_body.as_ref(),
        "llm doctor completion probe",
    )?;

    let start = Instant::now();

    let mut req = client.post(&completions_url).json(&request_body);
    if let Some(ref key) = api_key {
        // Don't leak the key over plaintext HTTP to a remote host / userinfo URL.
        if crate::config::api_key::assert_credential_endpoint_safe(&completions_url, true).is_ok() {
            req = req.bearer_auth(key);
        } else {
            eprintln!("  ⚠ not sending API key to unsafe endpoint {completions_url}");
        }
    }

    let resp = req
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Connection test failed: {}", e))?;

    let latency = start.elapsed();

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Completion request returned HTTP {}: {}", status, body);
    }

    let body: Value = resp.json().await?;
    let tokens_per_second = extract_tokens_per_second(&body, latency);

    // Test tool calling
    let tool_calling_works =
        test_tool_calling(&client, &completions_url, model, api_key.as_deref(), config).await?;

    Ok(ConnectionTestResult {
        latency,
        tokens_per_second,
        tool_calling_works,
    })
}

fn extract_tokens_per_second(body: &Value, latency: Duration) -> Option<f64> {
    // Try usage.completion_tokens
    let completion_tokens = body
        .get("usage")
        .and_then(|u| u.get("completion_tokens"))
        .and_then(|t| t.as_u64())?;

    let secs = latency.as_secs_f64();
    if secs > 0.0 && completion_tokens > 0 {
        Some(completion_tokens as f64 / secs)
    } else {
        None
    }
}

async fn test_tool_calling(
    client: &Client,
    completions_url: &str,
    model: &str,
    api_key: Option<&str>,
    config: &Config,
) -> Result<Option<bool>> {
    let mut request_body = serde_json::json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": "When a suitable tool is provided, call it instead of answering directly."
            },
            {"role": "user", "content": "What is 2 + 2? Use the calculator tool."}
        ],
        "tools": [
            {
                "type": "function",
                "function": {
                    "name": "calculator",
                    "description": "Perform arithmetic calculations",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "expression": {
                                "type": "string",
                                "description": "The arithmetic expression to evaluate"
                            }
                        },
                        "required": ["expression"]
                    }
                }
            }
        ],
        "max_tokens": 128,
        "temperature": 0.0
    });
    merge_extra_body(
        &mut request_body,
        config.extra_body.as_ref(),
        "llm doctor tool-calling probe",
    )?;

    let mut req = client
        .post(completions_url)
        .timeout(connection_test_timeout(config))
        .json(&request_body);
    if let Some(key) = api_key {
        // Don't leak the key over plaintext HTTP to a remote host / userinfo URL.
        if crate::config::api_key::assert_credential_endpoint_safe(completions_url, true).is_ok() {
            req = req.bearer_auth(key);
        } else {
            eprintln!("  ⚠ not sending API key to unsafe endpoint {completions_url}");
        }
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };

    if !resp.status().is_success() {
        return Ok(Some(false));
    }

    let body: Value = match resp.json().await {
        Ok(b) => b,
        Err(_) => return Ok(Some(false)),
    };

    // Check if the response contains native tool_calls
    let has_tool_calls = body
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|msg| msg.get("tool_calls"))
        .and_then(|tc| tc.as_array())
        .is_some_and(|arr| !arr.is_empty());

    // GLM-5.2, Qwen, and other text-format models emit tool calls as
    // text/XML inside the `content` field rather than as native `tool_calls`.
    // If there are no native tool_calls, also check whether the response
    // content contains a parseable text/XML tool call before concluding that
    // tool calling is broken.
    if !has_tool_calls {
        let content = body
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("");

        if !content.is_empty() {
            let parsed = crate::tool_parser::parse_tool_calls(content);
            if !parsed.tool_calls.is_empty() {
                return Ok(Some(true));
            }
        }
        return Ok(Some(false));
    }

    Ok(Some(has_tool_calls))
}

// ── Step 6 implementation ────────────────────────────────────────────────────

fn print_recommendations(
    det: &DetectionResult,
    config: &Config,
    conn: Option<&ConnectionTestResult>,
) {
    let model_name = config.model.as_str();
    // Find the model in the list
    let model_info = det
        .models
        .iter()
        .find(|m| m.id == model_name || m.id.contains(model_name));

    let ctx_str = model_info
        .and_then(|m| m.max_model_len)
        .map(|l| format!("{} tokens", l))
        .unwrap_or_else(|| "unknown".to_string());

    let backend_str = det.backend.to_string();
    let model_display = model_info.map(|m| m.id.as_str()).unwrap_or(model_name);

    // Collect recommendations
    let mut checks: Vec<(CheckStatus, String)> = Vec::new();

    // Context length check
    if let Some(info) = model_info {
        if let Some(ctx) = info.max_model_len {
            if ctx >= MIN_RECOMMENDED_CONTEXT {
                checks.push((CheckStatus::Ok, "Context length is sufficient".to_string()));
            } else {
                checks.push((
                    CheckStatus::Warn,
                    format!(
                        "Context length ({}) is below recommended ({})",
                        ctx, MIN_RECOMMENDED_CONTEXT
                    ),
                ));
            }

            if config.context_length < ctx as usize {
                checks.push((
                    CheckStatus::Info,
                    format!(
                        "Raise selfware context_length from {} to {} to use the full backend window",
                        config.context_length, ctx
                    ),
                ));
            } else if config.context_length > ctx as usize {
                checks.push((
                    CheckStatus::Warn,
                    format!(
                        "selfware context_length ({}) exceeds backend max_model_len ({})",
                        config.context_length, ctx
                    ),
                ));
            }
        }
    }

    // Tool calling check
    if let Some(c) = conn {
        match c.tool_calling_works {
            Some(true) => {
                checks.push((CheckStatus::Ok, "Tool calling supported".to_string()));
            }
            Some(false) => {
                checks.push((
                    CheckStatus::Warn,
                    "Tool calling did not produce tool_calls — check chat template".to_string(),
                ));
            }
            None => {}
        }

        // Latency check
        if c.latency.as_millis() > 10_000 {
            checks.push((
                CheckStatus::Warn,
                "High latency — consider a faster backend or smaller model".to_string(),
            ));
        }
    }

    // Backend-specific recommendations
    match det.backend {
        Backend::Sglang => {
            checks.push((
                CheckStatus::Info,
                "Consider enabling --enable-torch-compile for better throughput".to_string(),
            ));
            if is_qwen_model(model_name) {
                match configured_enable_thinking(config) {
                    Some(false) => checks.push((
                        CheckStatus::Ok,
                        "Qwen/SGLang thinking is disabled in selfware extra_body".to_string(),
                    )),
                    Some(true) => checks.push((
                        CheckStatus::Warn,
                        "Disable chat_template_kwargs.enable_thinking for Qwen/SGLang tool workflows"
                            .to_string(),
                    )),
                    None => checks.push((
                        CheckStatus::Warn,
                        "Add chat_template_kwargs.enable_thinking = false to match the runtime path"
                            .to_string(),
                    )),
                }
            }
            if model_name.to_lowercase().contains("vision")
                || model_name.to_lowercase().contains("vl")
            {
                checks.push((
                    CheckStatus::Info,
                    "For visual tasks, add --served-model-name".to_string(),
                ));
            }
        }
        Backend::Vllm => {
            checks.push((
                CheckStatus::Info,
                "Consider --enable-prefix-caching for repeated prompts".to_string(),
            ));
        }
        Backend::Ollama if is_qwen_model(model_name) => {
            checks.push((
                CheckStatus::Info,
                "Set OLLAMA_NUM_PARALLEL=1 for best single-request throughput".to_string(),
            ));
        }
        Backend::LlamaCpp => {
            checks.push((
                CheckStatus::Info,
                "Consider --mlock to prevent model from swapping to disk".to_string(),
            ));
        }
        _ => {}
    }

    // Print the box
    let width = 52;
    let border_top = format!(
        "{}{}{}",
        "+-".cyan(),
        " LLM Configuration Recommendations ".cyan().bold(),
        "-+".cyan()
    );
    let border_bot = format!(
        "{}",
        "+-----------------------------------------------------+".cyan()
    );

    println!("{}", border_top);
    println!(
        "{}",
        "|                                                     |".cyan()
    );
    println!(
        "{} Backend: {:<width$}{}",
        "|".cyan(),
        backend_str,
        "|".cyan(),
        width = width - 11
    );
    println!(
        "{} Model: {:<width$}{}",
        "|".cyan(),
        truncate_str(model_display, width - 10),
        "|".cyan(),
        width = width - 9
    );
    println!(
        "{} Context: {:<width$}{}",
        "|".cyan(),
        ctx_str,
        "|".cyan(),
        width = width - 11
    );
    println!(
        "{}",
        "|                                                     |".cyan()
    );

    for (status, msg) in &checks {
        let (icon, colored_msg) = match status {
            CheckStatus::Ok => ("ok".green().to_string(), msg.green().to_string()),
            CheckStatus::Warn => ("!!".yellow().to_string(), msg.yellow().to_string()),
            CheckStatus::Info => (">>".cyan().to_string(), msg.cyan().to_string()),
        };
        println!(
            "{} {} {:<width$}{}",
            "|".cyan(),
            icon,
            colored_msg,
            "|".cyan(),
            width = width - 6
        );
    }

    println!(
        "{}",
        "|                                                     |".cyan()
    );
    println!("{}", border_bot);
}

#[derive(Debug)]
enum CheckStatus {
    Ok,
    Warn,
    Info,
}

fn connection_test_timeout(config: &Config) -> Duration {
    Duration::from_secs(config.agent.step_timeout_secs.clamp(
        MIN_CONNECTION_TEST_TIMEOUT_SECS,
        MAX_CONNECTION_TEST_TIMEOUT_SECS,
    ))
}

fn configured_enable_thinking(config: &Config) -> Option<bool> {
    config
        .extra_body
        .as_ref()?
        .get("chat_template_kwargs")?
        .as_object()?
        .get("enable_thinking")?
        .as_bool()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "../tests/unit/llm_doctor/llm_doctor_test.rs"]
mod tests;
