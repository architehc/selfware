//! Unpack — automatic local LLM discovery and setup via llmfit.
//!
//! When selfware starts without a valid config or reachable endpoint,
//! `unpack()` scans for local LLM servers (LM Studio, Ollama, etc.),
//! detects the best available model, and generates a matching Config.
//! If nothing is running, it uses llmfit-core to recommend and optionally
//! download a model that fits the user's hardware.

use anyhow::Result;
use colored::Colorize;
use llmfit_core::{
    fit::{rank_models_by_fit, FitLevel, ModelFit},
    hardware::SystemSpecs,
    models::{Capability, ModelDatabase, UseCase},
    providers::{LmStudioProvider, OllamaProvider},
};
use tracing::{info, warn};

use crate::config::{auto_config::AutoConfigurator, Config};

/// Information about a discovered local model endpoint.
#[derive(Debug, Clone)]
pub struct DiscoveredEndpoint {
    pub provider: String,
    pub endpoint: String,
    pub model: String,
    pub context_length: usize,
    pub multimodal: bool,
}

/// Scan common local LLM endpoints and return any that are reachable.
pub async fn scan_local_endpoints() -> Vec<DiscoveredEndpoint> {
    let mut results = Vec::new();

    // LM Studio default
    if let Some(ep) = probe_lmstudio().await {
        results.push(ep);
    }

    // Ollama OpenAI-compatible
    if let Some(ep) = probe_ollama().await {
        results.push(ep);
    }

    // Generic local servers on common ports
    for port in [8000u16, 8080, 3000] {
        if let Some(ep) = probe_generic(port).await {
            results.push(ep);
        }
    }

    results
}

async fn probe_lmstudio() -> Option<DiscoveredEndpoint> {
    let provider = LmStudioProvider::new();
    let (available, models, _count) = provider.detect_with_installed();
    if !available || models.is_empty() {
        return None;
    }

    let endpoint = "http://127.0.0.1:1234/v1".to_string();
    let model = models.iter().next().cloned()?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .ok()?;

    let resp = client
        .get(format!("{}/models", endpoint))
        .send()
        .await
        .ok()?;

    let body = resp.json::<serde_json::Value>().await.ok()?;

    let model_info = body
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|arr| {
            arr.iter().find(|m| {
                m.get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_lowercase() == model)
                    .unwrap_or(false)
            })
        })?;

    let context_length = model_info
        .get("max_model_len")
        .and_then(|v| v.as_u64())
        .or_else(|| model_info.get("context_length").and_then(|v| v.as_u64()))
        .unwrap_or(131072) as usize;

    let multimodal = is_multimodal(&model, model_info);

    Some(DiscoveredEndpoint {
        provider: "LM Studio".to_string(),
        endpoint,
        model,
        context_length,
        multimodal,
    })
}

async fn probe_ollama() -> Option<DiscoveredEndpoint> {
    let provider = OllamaProvider::new();
    let (available, models, _count) = provider.detect_with_installed();
    if !available || models.is_empty() {
        return None;
    }

    let endpoint = "http://localhost:11434/v1".to_string();
    let model = models.iter().next().cloned()?;

    let context_length = infer_context_length(&model) as usize;
    let multimodal = is_multimodal_by_name(&model);

    Some(DiscoveredEndpoint {
        provider: "Ollama".to_string(),
        endpoint,
        model,
        context_length,
        multimodal,
    })
}

async fn probe_generic(port: u16) -> Option<DiscoveredEndpoint> {
    let endpoint = format!("http://localhost:{}/v1", port);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .ok()?;

    let resp = client
        .get(format!("{}/models", endpoint))
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let body = resp.json::<serde_json::Value>().await.ok()?;
    let models = body.get("data").and_then(|d| d.as_array())?;
    let first = models.first()?;
    let model = first.get("id")?.as_str()?.to_string();

    let context_length = first
        .get("max_model_len")
        .and_then(|v| v.as_u64())
        .or_else(|| first.get("context_length").and_then(|v| v.as_u64()))
        .unwrap_or_else(|| infer_context_length(&model)) as usize;

    let multimodal = is_multimodal(&model, first);

    Some(DiscoveredEndpoint {
        provider: format!("Generic (port {})", port),
        endpoint,
        model,
        context_length,
        multimodal,
    })
}

fn is_multimodal(model: &str, info: &serde_json::Value) -> bool {
    if is_multimodal_by_name(model) {
        return true;
    }
    info.get("capabilities")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter().any(|v| {
                v.as_str()
                    .map(|s| s.contains("vision") || s.contains("multimodal"))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn is_multimodal_by_name(model: &str) -> bool {
    let lower = model.to_lowercase();
    lower.contains("vision")
        || lower.contains("-vl-")
        || lower.ends_with("-vl")
        || lower.contains("llava")
        || lower.contains("onevision")
        || lower.contains("pixtral")
        || (lower.contains("gemma-3") && lower.contains("it"))
}

fn infer_context_length(model: &str) -> u64 {
    let lower = model.to_lowercase();
    if lower.contains("128k") || lower.contains("131072") {
        131072
    } else if lower.contains("32k") || lower.contains("32768") {
        32768
    } else if lower.contains("8k") || lower.contains("8192") {
        8192
    } else if lower.contains("qwen3.5") || lower.contains("qwen3-5") || lower.contains("qwen3-coder") {
        131072
    } else if lower.contains("llama-3") || lower.contains("llama3") {
        131072
    } else {
        32768
    }
}

/// Run the full unpack routine: discover local endpoints, run auto-config,
/// or fall back to llmfit-based hardware recommendations.
pub async fn unpack() -> Result<Option<Config>> {
    println!(
        "\n{}",
        "🔍 Unpack — scanning for local LLM servers...".bold().cyan()
    );

    let endpoints = scan_local_endpoints().await;

    if let Some(best) = endpoints.first() {
        println!(
            "  {} Found {} running at {} with model {}",
            "✓".green(),
            best.provider.bright_white(),
            best.endpoint.dimmed(),
            best.model.bright_white()
        );

        let mut config = generate_config_for_endpoint(best).await?;

        // Update default model profile with modalities
        if let Some(profile) = config.models.get_mut("default") {
            if best.multimodal && !profile.modalities.contains(&"vision".to_string()) {
                profile.modalities.push("vision".to_string());
            }
            profile.context_length = best.context_length;
        }

        println!(
            "  {} Context: {} tokens | Multimodal: {} | Template: auto-detected\n",
            "ℹ".cyan(),
            best.context_length.to_string().bright_white(),
            if best.multimodal { "yes".green() } else { "no".dimmed() }
        );

        return Ok(Some(config));
    }

    println!(
        "  {} No local LLM server found. Analysing your hardware...",
        "!".yellow()
    );

    // Hardware-based recommendation via llmfit-core
    let specs = SystemSpecs::detect();
    println!(
        "     CPU: {} | RAM: {:.1} GB | GPU: {} | VRAM: {} GB",
        specs.cpu_name.dimmed(),
        specs.total_ram_gb,
        specs.gpu_name.as_deref().unwrap_or("none").dimmed(),
        specs
            .gpu_vram_gb
            .map(|v| format!("{:.1}", v))
            .unwrap_or_else(|| "N/A".to_string())
    );

    let db = ModelDatabase::new();
    let system = SystemSpecs::detect();
    let fits: Vec<ModelFit> = db
        .get_all_models()
        .iter()
        .map(|m| ModelFit::analyze(m, &system))
        .collect();

    let ranked = rank_models_by_fit(fits);
    let coding_models: Vec<&ModelFit> = ranked
        .iter()
        .filter(|f| {
            f.fit_level != FitLevel::TooTight
                && (f.use_case == UseCase::Coding || f.use_case == UseCase::General)
        })
        .take(5)
        .collect();

    if coding_models.is_empty() {
        println!(
            "  {} No suitable coding model found for your hardware.",
            "✗".red()
        );
        println!("     Try freeing up RAM or using a machine with more resources.\n");
        return Ok(None);
    }

    println!(
        "\n  {} Top recommended models for your hardware:\n",
        "★".yellow()
    );
    for (i, fit) in coding_models.iter().enumerate() {
        let caps = Capability::infer(&fit.model);
        let vision = if caps.contains(&Capability::Vision) {
            "vision".green()
        } else {
            "text".dimmed()
        };
        println!(
            "     {}. {} — {} tokens | {} | {:.0}% fit | {:.1} tok/s",
            i + 1,
            fit.model.name.bright_white(),
            fit.model.context_length.to_string().dimmed(),
            vision,
            fit.utilization_pct,
            fit.estimated_tps
        );
    }

    let top = coding_models.first().unwrap();
    println!(
        "\n  {} Best pick: {} (context: {}, quant: {})",
        "→".cyan(),
        top.model.name.bright_white().bold(),
        top.model.context_length,
        top.model.quantization.dimmed()
    );

    // Suggest how to get it running
    println!(
        "\n  {} To download and run this model automatically:\n",
        "💡".yellow()
    );
    println!(
        "     1. Install llmfit: {} (already integrated into selfware)",
        "cargo install llmfit".bright_white()
    );
    println!(
        "     2. Launch llmfit TUI and press 'd' on the highlighted model to download."
    );
    println!(
        "     3. Or run: {}\n",
        "llmfit recommend --use-case coding".bright_white()
    );

    // Still return a best-effort config pointing at the default LM Studio endpoint
    // so that the user can start selfware once they've loaded a model.
    let mut config = Config::default();
    config.endpoint = "http://127.0.0.1:1234/v1".to_string();
    config.model = top.model.name.clone();
    config.context_length = top.model.context_length as usize;

    let caps = Capability::infer(&top.model);
    let mut modalities = vec!["text".to_string()];
    if caps.contains(&Capability::Vision) {
        modalities.push("vision".to_string());
    }

    if let Some(profile) = config.models.get_mut("default") {
        profile.model = top.model.name.clone();
        profile.context_length = top.model.context_length as usize;
        profile.modalities = modalities;
    }

    Ok(Some(config))
}

async fn generate_config_for_endpoint(endpoint: &DiscoveredEndpoint) -> Result<Config> {
    let cfg = AutoConfigurator::new(&endpoint.endpoint, None);
    let mut config = cfg.generate_config(&endpoint.model).await?;

    // Override context length with what we discovered
    config.context_length = endpoint.context_length;

    // If the endpoint is LM Studio, update the default endpoint to prefer it
    if endpoint.provider == "LM Studio" {
        info!("Auto-configuring for LM Studio backend");
    }

    Ok(config)
}

/// Attempt to auto-unpack into the provided config if it looks unconfigured.
/// Returns `true` if the config was modified.
pub async fn try_auto_unpack(config: &mut Config) -> Result<bool> {
    // If the user explicitly set an endpoint other than defaults, don't override
    let is_default_endpoint = config.endpoint == super::default_endpoint()
        || config.endpoint == "http://localhost:8000/v1"
        || config.endpoint == "http://127.0.0.1:1234/v1";

    let is_default_model = config.model == super::default_model();

    if !is_default_endpoint && !is_default_model {
        // Looks explicitly configured — skip auto-unpack
        return Ok(false);
    }

    // Try to discover a running local server
    let endpoints = scan_local_endpoints().await;

    if let Some(best) = endpoints.first() {
        info!(
            "Auto-unpack discovered {} at {} with model {}",
            best.provider, best.endpoint, best.model
        );

        let cfg = AutoConfigurator::new(&best.endpoint, None);
        let detected = cfg.generate_config(&best.model).await?;

        config.endpoint = detected.endpoint;
        config.model = detected.model;
        config.max_tokens = detected.max_tokens;
        config.context_length = best.context_length;
        config.temperature = detected.temperature;
        config.agent.native_function_calling = detected.agent.native_function_calling;
        config.agent.streaming = detected.agent.streaming;
        config.agent.token_budget = detected.agent.token_budget;
        config.extra_body = detected.extra_body.clone();

        // Update default profile modalities
        if let Some(profile) = config.models.get_mut("default") {
            profile.endpoint = config.endpoint.clone();
            profile.model = config.model.clone();
            profile.max_tokens = config.max_tokens;
            profile.temperature = config.temperature;
            profile.context_length = config.context_length;
            profile.extra_body = config.extra_body.clone();
            if best.multimodal && !profile.modalities.contains(&"vision".to_string()) {
                profile.modalities.push("vision".to_string());
            }
        }

        println!(
            "{} Auto-connected to {} ({}) — context: {}, multimodal: {}",
            "✓".green(),
            best.provider.bright_white(),
            best.endpoint.dimmed(),
            best.context_length.to_string().bright_white(),
            if best.multimodal { "yes".green() } else { "no".dimmed() }
        );

        return Ok(true);
    }

    // No running server found — fall back to llmfit recommendation
    warn!("No local LLM server found during auto-unpack");

    let specs = SystemSpecs::detect();
    let db = ModelDatabase::new();
    let fits: Vec<ModelFit> = db
        .get_all_models()
        .iter()
        .map(|m| ModelFit::analyze(m, &specs))
        .collect();

    let ranked = rank_models_by_fit(fits);
    if let Some(top) = ranked.iter().find(|f| {
        f.fit_level != FitLevel::TooTight
            && (f.use_case == UseCase::Coding || f.use_case == UseCase::General)
    }) {
        info!(
            "llmfit recommends {} as the best local model",
            top.model.name
        );

        config.model = top.model.name.clone();
        config.context_length = top.model.context_length as usize;
        if config.endpoint == super::default_endpoint() {
            // Prefer LM Studio endpoint since that's what the user asked for
            config.endpoint = "http://127.0.0.1:1234/v1".to_string();
        }

        let caps = Capability::infer(&top.model);
        let mut modalities = vec!["text".to_string()];
        if caps.contains(&Capability::Vision) {
            modalities.push("vision".to_string());
        }

        if let Some(profile) = config.models.get_mut("default") {
            profile.endpoint = config.endpoint.clone();
            profile.model = config.model.clone();
            profile.context_length = config.context_length;
            profile.modalities = modalities.clone();
        }

        println!(
            "{} No local server detected. llmfit recommends: {} (context: {}, fit: {:.0}%)",
            "ℹ".yellow(),
            top.model.name.bright_white(),
            top.model.context_length.to_string().bright_white(),
            top.utilization_pct
        );
        println!(
            "  {} Install a local backend (e.g. LM Studio on {}) and load this model to connect.\n",
            "→".dimmed(),
            config.endpoint.dimmed()
        );

        return Ok(true);
    }

    Ok(false)
}

/// Save the given config to `selfware.toml` in the current directory.
pub fn save_unpack_config(config: &Config) -> Result<std::path::PathBuf> {
    let path = std::path::PathBuf::from("selfware.toml");

    let mut extra_body_str = String::new();
    if let Some(ref extra) = config.extra_body {
        extra_body_str.push_str("\n[extra_body]\n");
        for (k, v) in extra {
            if let Some(obj) = v.as_object() {
                let inner: Vec<String> =
                    obj.iter().map(|(ik, iv)| format!("{ik} = {iv}")).collect();
                extra_body_str.push_str(&format!("{k} = {{ {} }}\n", inner.join(", ")));
            } else {
                extra_body_str.push_str(&format!("{k} = {v}\n"));
            }
        }
    }

    let content = format!(
        r#"# Selfware Configuration — auto-generated by unpack
endpoint = "{}"
model = "{}"
max_tokens = {}
context_length = {}
temperature = {}

[safety]
allowed_paths = ["./**", "/tmp/**"]
denied_paths = ["**/.env", "**/secrets/**", "**/.ssh/**"]
protected_branches = ["main"]

[agent]
native_function_calling = {}
streaming = {}
token_budget = {}
step_timeout_secs = {}
{}
"#,
        config.endpoint,
        config.model,
        config.max_tokens,
        config.context_length,
        config.temperature,
        config.agent.native_function_calling,
        config.agent.streaming,
        config.agent.token_budget,
        config.agent.step_timeout_secs,
        extra_body_str
    );

    std::fs::write(&path, content)?;
    Ok(path)
}
