//! LLM Doctor — diagnostic module for local LLM backend configuration.
//!
//! Detects the backend type (sglang, vllm, ollama, llama.cpp, lmstudio),
//! analyses model capabilities, checks context length and chat templates,
//! runs a connectivity/latency test, and prints a recommendations tree.

use anyhow::Result;
use colored::Colorize;
use reqwest::Client;
use serde_json::Value;
use std::time::{Duration, Instant};

use crate::api::merge_extra_body;
use crate::config::Config;

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

// ── Public entry-point ───────────────────────────────────────────────────────

/// Run the full LLM doctor diagnostic.
pub async fn run_llm_doctor(config: &Config) -> Result<()> {
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

    // Step 1: Detect Backend
    println!("{}", "Step 1: Detecting Backend".bold().underline());
    let detection = detect_backend(&endpoint).await;

    match &detection {
        Ok(det) => {
            println!(
                "  {} Endpoint: {}",
                ">>".green(),
                det.endpoint.bright_white()
            );
            println!(
                "  {} Backend:  {}",
                ">>".green(),
                det.backend.to_string().bright_yellow()
            );
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
        }
        Err(e) => {
            println!(
                "  {} Could not reach endpoint: {}",
                "!!".red().bold(),
                endpoint.bright_white()
            );
            println!("     {}", e.to_string().red());
            println!();
            println!(
                "  {} Make sure your LLM backend is running and the endpoint",
                ">>".yellow()
            );
            println!("     in selfware.toml is correct.");
            println!();
            return Ok(());
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

    Ok(())
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

    let body: Value = resp.json().await?;

    // Parse model list
    let models = parse_models(&body);

    // Detect backend by probing various signals
    let backend = identify_backend(&client, base_no_v1, &body).await;

    Ok(DetectionResult {
        backend,
        models,
        endpoint: endpoint.to_string(),
    })
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
        req = req.bearer_auth(key);
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
        req = req.bearer_auth(key);
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

    // Check if the response contains tool_calls
    let has_tool_calls = body
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|msg| msg.get("tool_calls"))
        .and_then(|tc| tc.as_array())
        .is_some_and(|arr| !arr.is_empty());

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
        Backend::Ollama => {
            if is_qwen_model(model_name) {
                checks.push((
                    CheckStatus::Info,
                    "Set OLLAMA_NUM_PARALLEL=1 for best single-request throughput".to_string(),
                ));
            }
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

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
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
mod tests {
    use super::*;

    #[test]
    fn test_is_qwen35_model() {
        assert!(is_qwen35_model("Qwen3.5-122B-A10B"));
        assert!(is_qwen35_model("qwen3.5-32b"));
        assert!(is_qwen35_model("Qwen3-5-122B"));
        assert!(!is_qwen35_model("Qwen3-Coder"));
        assert!(!is_qwen35_model("llama-3"));
    }

    #[test]
    fn test_is_qwen_model() {
        assert!(is_qwen_model("Qwen3.5-122B-A10B"));
        assert!(is_qwen_model("Qwen/Qwen3-Coder-Next-FP8"));
        assert!(!is_qwen_model("llama-3.1-70b"));
    }

    #[test]
    fn test_is_model_small() {
        assert!(is_model_small("qwen-7b"));
        assert!(is_model_small("llama-3b-instruct"));
        assert!(is_model_small("phi-2b"));
        assert!(!is_model_small("qwen-72b"));
        assert!(!is_model_small("qwen3.5-122b"));
        assert!(!is_model_small("qwen-14b"));
    }

    #[test]
    fn test_parse_models_empty() {
        let body = serde_json::json!({"data": []});
        let models = parse_models(&body);
        assert!(models.is_empty());
    }

    #[test]
    fn test_parse_models_with_data() {
        let body = serde_json::json!({
            "data": [
                {
                    "id": "Qwen/Qwen3.5-122B-A10B",
                    "max_model_len": 131072
                },
                {
                    "id": "other-model",
                    "context_length": 8192
                }
            ]
        });
        let models = parse_models(&body);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "Qwen/Qwen3.5-122B-A10B");
        assert_eq!(models[0].max_model_len, Some(131072));
        assert_eq!(models[1].id, "other-model");
        assert_eq!(models[1].max_model_len, Some(8192));
    }

    #[test]
    fn test_configured_enable_thinking_false() {
        let mut config = Config::default();
        config.extra_body = Some({
            let mut extra = serde_json::Map::new();
            extra.insert(
                "chat_template_kwargs".to_string(),
                serde_json::json!({ "enable_thinking": false }),
            );
            extra
        });

        assert_eq!(configured_enable_thinking(&config), Some(false));
    }

    #[test]
    fn test_configured_enable_thinking_missing() {
        let config = Config::default();
        assert_eq!(configured_enable_thinking(&config), None);
    }

    #[test]
    fn test_connection_test_timeout_respects_minimum() {
        let mut config = Config::default();
        config.agent.step_timeout_secs = 5;
        assert_eq!(
            connection_test_timeout(&config),
            Duration::from_secs(MIN_CONNECTION_TEST_TIMEOUT_SECS)
        );
    }

    #[test]
    fn test_connection_test_timeout_respects_maximum() {
        let mut config = Config::default();
        config.agent.step_timeout_secs = 600;
        assert_eq!(
            connection_test_timeout(&config),
            Duration::from_secs(MAX_CONNECTION_TEST_TIMEOUT_SECS)
        );
    }

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world foo bar", 10), "hello w...");
    }

    #[test]
    fn test_backend_display() {
        assert_eq!(Backend::Sglang.to_string(), "sglang");
        assert_eq!(Backend::Vllm.to_string(), "vllm");
        assert_eq!(Backend::Ollama.to_string(), "ollama");
        assert_eq!(Backend::LlamaCpp.to_string(), "llama.cpp");
        assert_eq!(Backend::LmStudio.to_string(), "lmstudio");
        assert_eq!(
            Backend::Unknown("test".to_string()).to_string(),
            "unknown (test)"
        );
    }

    #[test]
    fn test_extract_tokens_per_second() {
        let body = serde_json::json!({
            "usage": {
                "completion_tokens": 10
            }
        });
        let tps = extract_tokens_per_second(&body, Duration::from_secs(1));
        assert_eq!(tps, Some(10.0));

        let empty = serde_json::json!({});
        assert_eq!(
            extract_tokens_per_second(&empty, Duration::from_secs(1)),
            None
        );
    }

    // =========================================================================
    // is_model_small extended tests
    // =========================================================================

    #[test]
    fn test_is_model_small_1b() {
        assert!(is_model_small("model-1b"));
    }

    #[test]
    fn test_is_model_small_0_5b() {
        assert!(is_model_small("model-0.5b"));
    }

    #[test]
    fn test_is_model_small_1_5b() {
        assert!(is_model_small("model-1.5b"));
    }

    #[test]
    fn test_is_model_small_3b() {
        assert!(is_model_small("llama-3b-instruct"));
    }

    #[test]
    fn test_is_model_small_4b() {
        assert!(is_model_small("phi-4b"));
    }

    #[test]
    fn test_is_model_small_5b() {
        assert!(is_model_small("model_5b"));
    }

    #[test]
    fn test_is_model_small_6b() {
        assert!(is_model_small("chatglm-6b"));
    }

    #[test]
    fn test_is_model_small_7b_not_72b() {
        assert!(is_model_small("qwen-7b"));
        assert!(!is_model_small("qwen-72b"));
    }

    #[test]
    fn test_not_small_14b() {
        assert!(!is_model_small("qwen-14b"));
    }

    #[test]
    fn test_not_small_32b() {
        assert!(!is_model_small("qwen-32b"));
    }

    #[test]
    fn test_not_small_70b() {
        assert!(!is_model_small("llama-70b"));
    }

    #[test]
    fn test_not_small_122b() {
        assert!(!is_model_small("qwen3.5-122b"));
    }

    #[test]
    fn test_is_model_small_underscore_separator() {
        assert!(is_model_small("model_7b"));
        assert!(is_model_small("model_3b_instruct"));
    }

    // =========================================================================
    // is_qwen35_model extended tests
    // =========================================================================

    #[test]
    fn test_is_qwen35_lowercase() {
        assert!(is_qwen35_model("qwen3.5-27b"));
    }

    #[test]
    fn test_is_qwen35_mixed_case() {
        assert!(is_qwen35_model("QWEN3.5-122B-A10B"));
    }

    #[test]
    fn test_is_qwen35_dash_variant() {
        assert!(is_qwen35_model("qwen3-5-27b"));
    }

    #[test]
    fn test_not_qwen35_qwen2() {
        assert!(!is_qwen35_model("qwen2.5-72b"));
    }

    // =========================================================================
    // is_qwen_model extended tests
    // =========================================================================

    #[test]
    fn test_is_qwen_any_version() {
        assert!(is_qwen_model("Qwen2-72B"));
        assert!(is_qwen_model("qwen-1.5-7b"));
        assert!(is_qwen_model("Qwen3.5-122B"));
    }

    #[test]
    fn test_not_qwen_other_model() {
        assert!(!is_qwen_model("llama-3-70b"));
        assert!(!is_qwen_model("phi-3-mini"));
    }

    // =========================================================================
    // parse_models extended tests
    // =========================================================================

    #[test]
    fn test_parse_models_no_data_field() {
        let body = serde_json::json!({"other": "value"});
        let models = parse_models(&body);
        assert!(models.is_empty());
    }

    #[test]
    fn test_parse_models_data_not_array() {
        let body = serde_json::json!({"data": "string"});
        let models = parse_models(&body);
        assert!(models.is_empty());
    }

    #[test]
    fn test_parse_models_with_max_tokens_field() {
        let body = serde_json::json!({
            "data": [{"id": "model-1", "max_tokens": 4096}]
        });
        let models = parse_models(&body);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].max_model_len, Some(4096));
    }

    #[test]
    fn test_parse_models_no_context_info() {
        let body = serde_json::json!({
            "data": [{"id": "minimal-model"}]
        });
        let models = parse_models(&body);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "minimal-model");
        assert_eq!(models[0].max_model_len, None);
    }

    #[test]
    fn test_parse_models_missing_id() {
        let body = serde_json::json!({
            "data": [{"max_model_len": 8192}]
        });
        let models = parse_models(&body);
        assert_eq!(models[0].id, "unknown");
    }

    // =========================================================================
    // Backend display tests (extended)
    // =========================================================================

    #[test]
    fn test_backend_equality() {
        assert_eq!(Backend::Sglang, Backend::Sglang);
        assert_ne!(Backend::Sglang, Backend::Vllm);
        assert_ne!(Backend::Ollama, Backend::LlamaCpp);
        assert_eq!(
            Backend::Unknown("test".to_string()),
            Backend::Unknown("test".to_string())
        );
        assert_ne!(
            Backend::Unknown("a".to_string()),
            Backend::Unknown("b".to_string())
        );
    }

    #[test]
    fn test_backend_clone() {
        let b = Backend::Vllm;
        let cloned = b.clone();
        assert_eq!(b, cloned);
    }

    // =========================================================================
    // extract_tokens_per_second extended tests
    // =========================================================================

    #[test]
    fn test_extract_tps_zero_seconds() {
        let body = serde_json::json!({"usage": {"completion_tokens": 10}});
        let tps = extract_tokens_per_second(&body, Duration::from_secs(0));
        // 0 seconds -> secs_f64 == 0.0, condition `secs > 0.0` is false => None
        assert_eq!(tps, None);
    }

    #[test]
    fn test_extract_tps_zero_tokens() {
        let body = serde_json::json!({"usage": {"completion_tokens": 0}});
        let tps = extract_tokens_per_second(&body, Duration::from_secs(1));
        assert_eq!(tps, None);
    }

    #[test]
    fn test_extract_tps_no_usage() {
        let body = serde_json::json!({"choices": []});
        assert_eq!(
            extract_tokens_per_second(&body, Duration::from_secs(1)),
            None
        );
    }

    #[test]
    fn test_extract_tps_no_completion_tokens() {
        let body = serde_json::json!({"usage": {"prompt_tokens": 100}});
        assert_eq!(
            extract_tokens_per_second(&body, Duration::from_secs(1)),
            None
        );
    }

    // =========================================================================
    // truncate_str tests
    // =========================================================================

    #[test]
    fn test_truncate_str_short() {
        assert_eq!(truncate_str("hi", 10), "hi");
    }

    #[test]
    fn test_truncate_str_exact() {
        assert_eq!(truncate_str("12345", 5), "12345");
    }

    #[test]
    fn test_truncate_str_long() {
        assert_eq!(truncate_str("hello world", 8), "hello...");
    }

    #[test]
    fn test_truncate_str_very_short_max() {
        assert_eq!(truncate_str("hello", 3), "...");
    }

    // =========================================================================
    // ModelInfo tests
    // =========================================================================

    #[test]
    fn test_model_info_clone() {
        let info = ModelInfo {
            id: "test-model".to_string(),
            max_model_len: Some(131072),
            raw: serde_json::json!({}),
        };
        let cloned = info.clone();
        assert_eq!(cloned.id, "test-model");
        assert_eq!(cloned.max_model_len, Some(131072));
    }

    #[test]
    fn test_model_info_debug() {
        let info = ModelInfo {
            id: "model".to_string(),
            max_model_len: None,
            raw: serde_json::json!({}),
        };
        let s = format!("{:?}", info);
        assert!(s.contains("model"));
    }
}
