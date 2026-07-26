//! Unpack — zero-touch local LLM auto-calibration via llmfit.
//!
//! When selfware starts without a valid config or reachable endpoint,
//! `auto_calibrate()` does everything possible to get the user running:
//!   1. Scan for local LLM servers (LM Studio, Ollama, vLLM, etc.)
//!   2. If nothing is running, try to auto-start installed backends
//!   3. If a backend is running but empty, auto-pull a hardware-matched model
//!   4. Detect capabilities (context length, tool calling, multimodal, template)
//!   5. Generate and optionally persist a matching Config
//!   6. Fall back to llmfit hardware recommendations with actionable next steps

use anyhow::Result;
use colored::Colorize;
use llmfit_core::{
    fit::{rank_models_by_fit, FitLevel, ModelFit},
    hardware::SystemSpecs,
    models::{Capability, ModelDatabase, UseCase},
    providers::{LmStudioProvider, ModelProvider, OllamaProvider},
};
use tracing::warn;

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

    if let Some(ep) = probe_lmstudio().await {
        results.push(ep);
    }
    if let Some(ep) = probe_ollama().await {
        results.push(ep);
    }
    for port in [8000u16, 8080, 3000, 5000] {
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

    let body = client
        .get(format!("{}/models", endpoint))
        .send()
        .await
        .ok()?
        .json::<serde_json::Value>()
        .await
        .ok()?;

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
    let mut provider = OllamaProvider::new();
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
        || lower.contains("vl")
        || lower.ends_with("-vl")
        || lower.contains("llava")
        || lower.contains("onevision")
        || lower.contains("pixtral")
        || lower.contains("kimi")
        || lower.contains("gemini")
        || lower.contains("gpt-4o")
        || lower.contains("claude-3")
        || lower.contains("qvq")
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
    } else if lower.contains("qwen3.5")
        || lower.contains("qwen3-5")
        || lower.contains("qwen3-coder")
        || lower.contains("llama-3")
        || lower.contains("llama3")
    {
        131072
    } else {
        32768
    }
}

// ── Backend auto-start helpers ───────────────────────────────────────────────

fn is_ollama_installed() -> bool {
    std::process::Command::new("ollama")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn is_lm_studio_installed() -> bool {
    #[cfg(target_os = "macos")]
    {
        std::path::Path::new("/Applications/LM Studio.app").exists()
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("lmstudio")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
            || std::path::Path::new("/opt/lmstudio/lmstudio").exists()
    }
    #[cfg(target_os = "windows")]
    {
        dirs::home_dir()
            .map(|h| h.join("AppData/Local/LM-Studio/bin/LM Studio.exe").exists())
            .unwrap_or(false)
    }
}

/// Try to start Ollama in the background and wait for it to come online.
async fn try_start_ollama() -> bool {
    if !is_ollama_installed() {
        return false;
    }

    println!(
        "  {} Ollama is installed but not running. Starting it now...",
        "⟳".cyan()
    );

    let _ = tokio::process::Command::new("ollama")
        .arg("serve")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    // Wait up to 10s for Ollama to come online
    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let provider = OllamaProvider::new();
        if provider.is_available() {
            println!("  {} Ollama is now online!", "✓".green());
            return true;
        }
    }

    false
}

/// Auto-pull a model via Ollama.
async fn ollama_pull(model: &str) -> Result<bool> {
    println!(
        "  {} Pulling model {} via Ollama (this may take a few minutes)...",
        "↓".cyan(),
        model.bright_white()
    );

    let output = tokio::process::Command::new("ollama")
        .args(["pull", model])
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run ollama pull: {}", e))?;

    if output.status.success() {
        println!("  {} Model {} ready.", "✓".green(), model.bright_white());
        Ok(true)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ollama pull failed: {}", stderr)
    }
}

/// Safely detect system specs, catching any panics from sysinfo/hardware probes.
fn safe_detect_specs() -> Option<SystemSpecs> {
    use std::panic::AssertUnwindSafe;
    match std::panic::catch_unwind(AssertUnwindSafe(SystemSpecs::detect)) {
        Ok(specs) => Some(specs),
        Err(_) => {
            warn!("SystemSpecs::detect() panicked — hardware detection unavailable");
            None
        }
    }
}

/// Pick a good default model for Ollama based on hardware.
fn pick_ollama_model_for_hardware() -> &'static str {
    let specs = safe_detect_specs().unwrap_or_else(|| SystemSpecs {
        total_ram_gb: 16.0,
        available_ram_gb: 8.0,
        total_cpu_cores: 4,
        cpu_name: "unknown".to_string(),
        has_gpu: false,
        gpu_vram_gb: None,
        total_gpu_vram_gb: None,
        gpu_name: None,
        gpu_count: 0,
        unified_memory: false,
        backend: llmfit_core::hardware::GpuBackend::CpuX86,
        gpus: vec![],
        gpu_available_gb: None,
        cluster_mode: false,
        cluster_node_count: 0,
    });

    if specs.has_gpu {
        if specs.gpu_vram_gb.unwrap_or(0.0) >= 24.0 {
            "qwen3.5:32b"
        } else if specs.gpu_vram_gb.unwrap_or(0.0) >= 12.0 {
            "qwen3.5:14b"
        } else if specs.gpu_vram_gb.unwrap_or(0.0) >= 8.0 {
            "qwen3.5:7b"
        } else {
            "qwen3.5:4b"
        }
    } else if specs.total_ram_gb >= 24.0 {
        "qwen3.5:14b"
    } else if specs.total_ram_gb >= 16.0 {
        "qwen3.5:7b"
    } else {
        "qwen3.5:4b"
    }
}

/// Check whether a config file was actually loaded (as opposed to defaults).
fn has_config_file() -> bool {
    std::path::Path::new("selfware.toml").exists()
        || dirs::home_dir()
            .map(|h| h.join(".config/selfware/config.toml").exists())
            .unwrap_or(false)
}

// ── Public calibration API ───────────────────────────────────────────────────

/// Run the full unpack routine: discover local endpoints, auto-start backends,
/// auto-pull models, and generate a matching Config.
pub async fn unpack() -> Result<Option<Config>> {
    let mut cfg = Config::default();
    if auto_calibrate(&mut cfg).await? {
        Ok(Some(cfg))
    } else {
        Ok(None)
    }
}

/// Whether `key`'s effective value came from an explicit user source (a
/// config file, an env var, or a CLI arg) rather than a built-in default or
/// a previous auto-calibration. This — not value-equality with the built-in
/// defaults — decides "unconfigured": a user whose config legitimately
/// matches the shipped defaults (e.g. the README OpenRouter block) must not
/// have it hijacked back to localhost by a locally-detected backend.
fn provenance_is_user_set(config: &Config, key: &str) -> bool {
    matches!(
        config.sources.get(key),
        Some(super::provenance::ConfigSource::ConfigFile(_))
            | Some(super::provenance::ConfigSource::EnvVar(_))
            | Some(super::provenance::ConfigSource::CliArg(_))
    )
}

/// Aggressive auto-calibration: scan, start backends, pull models, detect,
/// and update `config`. Returns `true` if the config was modified.
pub async fn auto_calibrate(config: &mut Config) -> Result<bool> {
    // "Unconfigured" is decided by load PROVENANCE first: a value the user
    // set explicitly (config file / env var / CLI arg) is never overwritten,
    // even when it equals a built-in default — the README's OpenRouter setup
    // IS the default value set, and the old value-equality heuristic hijacked
    // it back to a detected localhost backend. Value-equality survives only
    // as a fallback for hand-built Configs that carry no provenance at all.
    let endpoint_user_set = provenance_is_user_set(config, "endpoint")
        || (config.endpoint != super::default_endpoint()
            && config.endpoint != "http://localhost:8000/v1"
            && config.endpoint != "http://127.0.0.1:1234/v1");
    let model_user_set =
        provenance_is_user_set(config, "model") || config.model != super::default_model();

    // Skip calibration if the user explicitly configured things
    if endpoint_user_set && model_user_set {
        return Ok(false);
    }

    println!(
        "\n{}",
        "🔧 Auto-calibrating local LLM setup...".bold().cyan()
    );

    // ── Step 1: scan for running servers ────────────────────────────────────
    let mut endpoints = scan_local_endpoints().await;

    // ── Step 2: if nothing found, try to auto-start Ollama ──────────────────
    if endpoints.is_empty() && try_start_ollama().await {
        endpoints = scan_local_endpoints().await;
    }

    // ── Step 3: if Ollama is running but empty, auto-pull a model ───────────
    if endpoints.is_empty() {
        let provider = OllamaProvider::new();
        if provider.is_available() {
            let model = pick_ollama_model_for_hardware();
            if ollama_pull(model).await.unwrap_or(false) {
                endpoints = scan_local_endpoints().await;
            }
        }
    }

    // ── Step 4: if we found a running server, auto-configure ────────────────
    if let Some(best) = endpoints.first() {
        println!(
            "  {} Connected to {} at {} — model: {}",
            "✓".green(),
            best.provider.bright_white(),
            best.endpoint.dimmed(),
            best.model.bright_white()
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

        // Bug fix: record auto-config provenance so `selfware config show`
        // surfaces these values as `[auto-config]` instead of `[default]`,
        // and so subsequent CLI overrides don't get silently superseded.
        for key in [
            "endpoint",
            "model",
            "max_tokens",
            "context_length",
            "temperature",
            "agent.native_function_calling",
            "agent.streaming",
            "agent.token_budget",
        ] {
            config
                .sources
                .set(key, super::provenance::ConfigSource::AutoConfig);
        }

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
            "  {} Context: {} tokens | Multimodal: {} | Tools: {} | Streaming: {}",
            "ℹ".cyan(),
            best.context_length.to_string().bright_white(),
            if best.multimodal {
                "yes".green()
            } else {
                "no".dimmed()
            },
            if config.agent.native_function_calling {
                "yes".green()
            } else {
                "no".dimmed()
            },
            if config.agent.streaming {
                "yes".green()
            } else {
                "no".dimmed()
            }
        );

        // Auto-save if no config file exists yet
        if !has_config_file() {
            match save_unpack_config(config) {
                Ok(path) => {
                    println!(
                        "  {} Auto-saved configuration to {}\n",
                        "💾".green(),
                        path.display().to_string().bright_white()
                    );
                }
                Err(e) => {
                    warn!("Failed to auto-save config: {}", e);
                }
            }
        } else {
            println!();
        }

        return Ok(true);
    }

    // ── Step 5: nothing is running — llmfit hardware analysis ───────────────
    warn!("No local LLM server found during auto-calibration");

    let specs = match safe_detect_specs() {
        Some(s) => s,
        None => {
            println!(
                "  {} Could not analyse hardware. Using conservative defaults.",
                "!".yellow()
            );
            return Ok(true);
        }
    };
    println!(
        "  {} No local server found. Analysing your hardware...",
        "!".yellow()
    );
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
    let fits: Vec<ModelFit> = db
        .get_all_models()
        .iter()
        .map(|m| ModelFit::analyze(m, &specs))
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

    if !coding_models.is_empty() {
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
        config.model = top.model.name.clone();
        config.context_length = top.model.context_length as usize;
        if config.endpoint == super::default_endpoint() {
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
            "\n  {} Best pick: {} (context: {}, quant: {})",
            "→".cyan(),
            top.model.name.bright_white().bold(),
            top.model.context_length,
            top.model.quantization.dimmed()
        );
    }

    // ── Step 6: actionable next steps based on what's installed ─────────────
    println!(
        "\n  {} Get up and running in under 60 seconds:\n",
        "🚀".bright_cyan()
    );

    if is_ollama_installed() {
        let model = pick_ollama_model_for_hardware();
        println!(
            "     {} Ollama is installed. Run this in another terminal:",
            "●".green()
        );
        println!(
            "       {}\n",
            format!("ollama pull {}", model).bright_white()
        );
    } else {
        println!(
            "     {} Install Ollama (fastest path to a working model):",
            "●".green()
        );
        println!(
            "       {}\n",
            "curl -fsSL https://ollama.com/install.sh | sh".bright_white()
        );
    }

    if is_lm_studio_installed() {
        println!(
            "     {} LM Studio is installed. Launch it and load a model,",
            "●".cyan()
        );
        println!("       then selfware will auto-detect it on the next run.\n");
    } else {
        println!(
            "     {} Or download LM Studio for a GUI experience:",
            "●".cyan()
        );
        println!(
            "       {}\n",
            "https://lmstudio.ai".bright_white().underline()
        );
    }

    println!(
        "     {} For more options, run: {}\n",
        "●".yellow(),
        "llmfit recommend --use-case coding".bright_white()
    );

    Ok(true)
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

    // Never write a config the loader would reject: validate the exact TOML
    // body the way a disk read would, and fail loudly instead of persisting
    // garbage (e.g. a zero context_length / token_budget).
    Config::validate_generated_toml(&content)?;

    std::fs::write(&path, content)?;
    Ok(path)
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)] // test config builders: default-then-tweak is clearer
#[path = "../../tests/unit/config/unpack/unpack_test.rs"]
mod tests;
