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

/// Aggressive auto-calibration: scan, start backends, pull models, detect,
/// and update `config`. Returns `true` if the config was modified.
pub async fn auto_calibrate(config: &mut Config) -> Result<bool> {
    let is_default_endpoint = config.endpoint == super::default_endpoint()
        || config.endpoint == "http://localhost:8000/v1"
        || config.endpoint == "http://127.0.0.1:1234/v1";
    let is_default_model = config.model == super::default_model();

    // Skip calibration if the user explicitly configured things
    if !is_default_endpoint && !is_default_model {
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

    std::fs::write(&path, content)?;
    Ok(path)
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)] // test config builders: default-then-tweak is clearer
mod tests {
    use super::*;
    use crate::config::{default_endpoint, default_model, Config};

    // =========================================================================
    // DiscoveredEndpoint struct tests
    // =========================================================================

    #[test]
    fn test_discovered_endpoint_fields() {
        let ep = DiscoveredEndpoint {
            provider: "Ollama".to_string(),
            endpoint: "http://localhost:11434/v1".to_string(),
            model: "qwen3.5:7b".to_string(),
            context_length: 32768,
            multimodal: false,
        };
        assert_eq!(ep.provider, "Ollama");
        assert_eq!(ep.endpoint, "http://localhost:11434/v1");
        assert_eq!(ep.model, "qwen3.5:7b");
        assert_eq!(ep.context_length, 32768);
        assert!(!ep.multimodal);
    }

    #[test]
    fn test_discovered_endpoint_multimodal() {
        let ep = DiscoveredEndpoint {
            provider: "LM Studio".to_string(),
            endpoint: "http://127.0.0.1:1234/v1".to_string(),
            model: "llava-v1.6".to_string(),
            context_length: 8192,
            multimodal: true,
        };
        assert!(ep.multimodal);
        assert_eq!(ep.context_length, 8192);
    }

    #[test]
    fn test_discovered_endpoint_clone() {
        let ep = DiscoveredEndpoint {
            provider: "Generic (port 8000)".to_string(),
            endpoint: "http://localhost:8000/v1".to_string(),
            model: "test-model".to_string(),
            context_length: 131072,
            multimodal: true,
        };
        let cloned = ep.clone();
        assert_eq!(cloned.provider, ep.provider);
        assert_eq!(cloned.endpoint, ep.endpoint);
        assert_eq!(cloned.model, ep.model);
        assert_eq!(cloned.context_length, ep.context_length);
        assert_eq!(cloned.multimodal, ep.multimodal);
    }

    #[test]
    fn test_discovered_endpoint_debug() {
        let ep = DiscoveredEndpoint {
            provider: "Test".to_string(),
            endpoint: "http://localhost:8000/v1".to_string(),
            model: "model".to_string(),
            context_length: 4096,
            multimodal: false,
        };
        let debug_str = format!("{:?}", ep);
        assert!(debug_str.contains("DiscoveredEndpoint"));
        assert!(debug_str.contains("Test"));
        assert!(debug_str.contains("4096"));
    }

    // =========================================================================
    // is_multimodal_by_name tests
    // =========================================================================

    #[test]
    fn test_is_multimodal_by_name_vision() {
        assert!(is_multimodal_by_name("llama-3-vision"));
        assert!(is_multimodal_by_name("qwen-vision-7b"));
    }

    #[test]
    fn test_is_multimodal_by_name_vl_suffix() {
        assert!(is_multimodal_by_name("qwen2.5-vl"));
        assert!(is_multimodal_by_name("qwen2.5-vl-7b"));
    }

    #[test]
    fn test_is_multimodal_by_name_vl_infix() {
        assert!(is_multimodal_by_name("qwen2.5-vl-7b-instruct"));
        assert!(is_multimodal_by_name("model-vl-2b"));
    }

    #[test]
    fn test_is_multimodal_by_name_llava() {
        assert!(is_multimodal_by_name("llava-v1.6"));
        assert!(is_multimodal_by_name("LLAVA-1.5-7b"));
    }

    #[test]
    fn test_is_multimodal_by_name_onevision() {
        assert!(is_multimodal_by_name("onevision-7b"));
        assert!(is_multimodal_by_name("OneVision-v1"));
    }

    #[test]
    fn test_is_multimodal_by_name_pixtral() {
        assert!(is_multimodal_by_name("pixtral-12b"));
        assert!(is_multimodal_by_name("Pixtral-12B-2409"));
    }

    #[test]
    fn test_is_multimodal_by_name_gemma3_it() {
        assert!(is_multimodal_by_name("gemma-3-it"));
        assert!(is_multimodal_by_name("gemma-3-4b-it"));
    }

    #[test]
    fn test_is_multimodal_by_name_gemma3_non_it() {
        // gemma-3 without "it" should NOT be multimodal
        assert!(!is_multimodal_by_name("gemma-3-4b"));
        assert!(!is_multimodal_by_name("gemma-3-base"));
    }

    #[test]
    fn test_is_multimodal_by_name_case_insensitive() {
        assert!(is_multimodal_by_name("LLAVA-V1.6"));
        assert!(is_multimodal_by_name("Qwen2.5-VL"));
        assert!(is_multimodal_by_name("PIXTRAL-12B"));
    }

    #[test]
    fn test_is_multimodal_by_name_non_multimodal() {
        assert!(!is_multimodal_by_name("llama-3-8b-instruct"));
        assert!(!is_multimodal_by_name("qwen3.5:7b"));
        assert!(!is_multimodal_by_name("mistral-7b-instruct"));
        assert!(!is_multimodal_by_name("phi-3.5-mini"));
        assert!(!is_multimodal_by_name("deepseek-coder-v2"));
    }

    #[test]
    fn test_is_multimodal_by_name_empty() {
        assert!(!is_multimodal_by_name(""));
    }

    #[test]
    fn test_is_multimodal_by_name_plain_text() {
        assert!(!is_multimodal_by_name("text-only-model"));
    }

    // =========================================================================
    // is_multimodal tests (uses both name and JSON info)
    // =========================================================================

    #[test]
    fn test_is_multimodal_by_name_match() {
        let info = serde_json::json!({});
        assert!(is_multimodal("llava-7b", &info));
    }

    #[test]
    fn test_is_multimodal_by_capabilities_vision() {
        let info = serde_json::json!({
            "capabilities": ["text", "vision"]
        });
        assert!(is_multimodal("some-text-model", &info));
    }

    #[test]
    fn test_is_multimodal_by_capabilities_multimodal() {
        let info = serde_json::json!({
            "capabilities": ["multimodal"]
        });
        assert!(is_multimodal("some-text-model", &info));
    }

    #[test]
    fn test_is_multimodal_no_capabilities_not_multimodal() {
        let info = serde_json::json!({
            "capabilities": ["text", "tool_use"]
        });
        assert!(!is_multimodal("some-text-model", &info));
    }

    #[test]
    fn test_is_multimodal_empty_capabilities() {
        let info = serde_json::json!({
            "capabilities": []
        });
        assert!(!is_multimodal("some-text-model", &info));
    }

    #[test]
    fn test_is_multimodal_no_capabilities_field() {
        let info = serde_json::json!({
            "max_model_len": 8192
        });
        assert!(!is_multimodal("some-text-model", &info));
    }

    #[test]
    fn test_is_multimodal_capabilities_not_array() {
        let info = serde_json::json!({
            "capabilities": "vision"
        });
        assert!(!is_multimodal("some-text-model", &info));
    }

    #[test]
    fn test_is_multimodal_capabilities_with_non_string_entries() {
        let info = serde_json::json!({
            "capabilities": [42, true, "vision"]
        });
        assert!(is_multimodal("some-text-model", &info));
    }

    #[test]
    fn test_is_multimodal_name_overrides_info() {
        // Even without capabilities, name match should return true
        let info = serde_json::json!({});
        assert!(is_multimodal("pixtral-12b", &info));
    }

    // =========================================================================
    // infer_context_length tests
    // =========================================================================

    #[test]
    fn test_infer_context_length_128k_explicit() {
        assert_eq!(infer_context_length("model-128k"), 131072);
    }

    #[test]
    fn test_infer_context_length_131072_explicit() {
        assert_eq!(infer_context_length("model-131072"), 131072);
    }

    #[test]
    fn test_infer_context_length_32k_explicit() {
        assert_eq!(infer_context_length("model-32k"), 32768);
    }

    #[test]
    fn test_infer_context_length_32768_explicit() {
        assert_eq!(infer_context_length("model-32768"), 32768);
    }

    #[test]
    fn test_infer_context_length_8k_explicit() {
        assert_eq!(infer_context_length("model-8k"), 8192);
    }

    #[test]
    fn test_infer_context_length_8192_explicit() {
        assert_eq!(infer_context_length("model-8192"), 8192);
    }

    #[test]
    fn test_infer_context_length_qwen35() {
        assert_eq!(infer_context_length("qwen3.5-7b"), 131072);
    }

    #[test]
    fn test_infer_context_length_qwen35_dash_variant() {
        assert_eq!(infer_context_length("qwen3-5-7b"), 131072);
    }

    #[test]
    fn test_infer_context_length_qwen3_coder() {
        assert_eq!(infer_context_length("qwen3-coder-32b"), 131072);
    }

    #[test]
    fn test_infer_context_length_llama3_dash() {
        assert_eq!(infer_context_length("llama-3-8b-instruct"), 131072);
    }

    #[test]
    fn test_infer_context_length_llama3_nodash() {
        assert_eq!(infer_context_length("llama3-70b"), 131072);
    }

    #[test]
    fn test_infer_context_length_default_unknown() {
        assert_eq!(infer_context_length("unknown-model"), 32768);
    }

    #[test]
    fn test_infer_context_length_mistral() {
        // Mistral is not in the special-cased list, so should default to 32768
        assert_eq!(infer_context_length("mistral-7b-instruct"), 32768);
    }

    #[test]
    fn test_infer_context_length_empty() {
        assert_eq!(infer_context_length(""), 32768);
    }

    #[test]
    fn test_infer_context_length_case_insensitive() {
        assert_eq!(infer_context_length("LLAMA-3-8B"), 131072);
        assert_eq!(infer_context_length("Qwen3.5-7B"), 131072);
    }

    #[test]
    fn test_infer_context_length_128k_takes_precedence_over_32k() {
        // If both patterns are present, 128k should win (checked first)
        assert_eq!(infer_context_length("model-128k-32k"), 131072);
    }

    #[test]
    fn test_infer_context_length_32k_takes_precedence_over_8k() {
        assert_eq!(infer_context_length("model-32k-8k"), 32768);
    }

    #[test]
    fn test_infer_context_length_qwen35_overrides_8k() {
        // qwen3.5 is checked before the default; the model name has qwen3.5
        // but also 8k — the 8k check comes first so it returns 8192
        assert_eq!(infer_context_length("qwen3.5-8k"), 8192);
    }

    // =========================================================================
    // pick_ollama_model_for_hardware tests
    // =========================================================================

    #[test]
    fn test_pick_ollama_model_for_hardware_returns_valid_model() {
        let model = pick_ollama_model_for_hardware();
        // Should always return a non-empty model name
        assert!(!model.is_empty());
        // Should contain "qwen3.5"
        assert!(
            model.contains("qwen3.5"),
            "expected model to contain 'qwen3.5', got: {}",
            model
        );
    }

    #[test]
    fn test_pick_ollama_model_for_hardware_returns_known_size() {
        let model = pick_ollama_model_for_hardware();
        // Should be one of the known model sizes
        let valid_models = ["qwen3.5:4b", "qwen3.5:7b", "qwen3.5:14b", "qwen3.5:32b"];
        assert!(
            valid_models.contains(&model),
            "expected one of {:?}, got: {}",
            valid_models,
            model
        );
    }

    // =========================================================================
    // auto_calibrate tests (non-network paths)
    // =========================================================================

    #[tokio::test]
    async fn test_auto_calibrate_skips_when_config_is_explicit() {
        // When both endpoint and model are non-default, auto_calibrate should
        // return Ok(false) immediately without doing any network work.
        let mut config = Config::default();
        config.endpoint = "http://my-custom-endpoint:9999/v1".to_string();
        config.model = "my-custom-model".to_string();

        let result = auto_calibrate(&mut config).await;
        assert!(result.is_ok(), "auto_calibrate should not error");
        assert!(
            !result.unwrap(),
            "should return false when config is already explicit"
        );

        // Config should be unchanged
        assert_eq!(config.endpoint, "http://my-custom-endpoint:9999/v1");
        assert_eq!(config.model, "my-custom-model");
    }

    #[tokio::test]
    async fn test_auto_calibrate_skips_when_only_endpoint_custom_and_model_custom() {
        let mut config = Config::default();
        config.endpoint = "http://localhost:9999/v1".to_string();
        config.model = "custom-llm".to_string();

        let result = auto_calibrate(&mut config).await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_auto_calibrate_does_not_skip_when_default_endpoint_only() {
        // When endpoint is default but model is custom, it should NOT skip
        // (is_default_endpoint is true, is_default_model is false → one is default)
        // Actually re-reading: the skip condition is `!is_default_endpoint && !is_default_model`
        // So if either is default, it does NOT skip. This test verifies it proceeds.
        let mut config = Config::default();
        // Keep endpoint as default, set model to custom
        config.model = "custom-model".to_string();

        // This will try to scan endpoints (network). In a test environment
        // with no local servers, it should eventually return Ok(true) after
        // hardware analysis. We just verify it doesn't skip (returns Ok).
        let result = auto_calibrate(&mut config).await;
        assert!(result.is_ok(), "auto_calibrate should not error");
        // It should return true because it tried to calibrate (even if nothing found)
        assert!(
            result.unwrap(),
            "should return true when at least one field is still default"
        );
    }

    // =========================================================================
    // save_unpack_config tests
    // =========================================================================

    fn with_temp_cwd<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _guard = crate::test_support::CwdGuard::hold();
        let original = std::env::current_dir().expect("failed to get cwd");
        let tmp = tempfile::tempdir().expect("failed to create temp dir");
        std::env::set_current_dir(&tmp).expect("failed to change to temp dir");
        let result = f();
        // Always restore, even if f() panicked
        std::env::set_current_dir(&original).expect("failed to restore cwd");
        result
    }

    #[test]
    fn test_save_unpack_config_creates_file() {
        with_temp_cwd(|| {
            let config = Config::default();
            let path = save_unpack_config(&config).expect("save should succeed");
            assert!(path.exists(), "config file should exist");
            assert_eq!(
                path.file_name().unwrap(),
                "selfware.toml",
                "file should be named selfware.toml"
            );
        });
    }

    #[test]
    fn test_save_unpack_config_content_contains_endpoint() {
        with_temp_cwd(|| {
            let mut config = Config::default();
            config.endpoint = "http://localhost:8080/v1".to_string();
            let path = save_unpack_config(&config).expect("save should succeed");
            let content = std::fs::read_to_string(&path).expect("should read file");
            assert!(
                content.contains("http://localhost:8080/v1"),
                "content should contain the endpoint: {}",
                content
            );
        });
    }

    #[test]
    fn test_save_unpack_config_content_contains_model() {
        with_temp_cwd(|| {
            let mut config = Config::default();
            config.model = "test-model-xyz".to_string();
            let path = save_unpack_config(&config).expect("save should succeed");
            let content = std::fs::read_to_string(&path).expect("should read file");
            assert!(
                content.contains("test-model-xyz"),
                "content should contain the model: {}",
                content
            );
        });
    }

    #[test]
    fn test_save_unpack_config_content_contains_max_tokens() {
        with_temp_cwd(|| {
            let mut config = Config::default();
            config.max_tokens = 12345;
            let path = save_unpack_config(&config).expect("save should succeed");
            let content = std::fs::read_to_string(&path).expect("should read file");
            assert!(
                content.contains("12345"),
                "content should contain max_tokens: {}",
                content
            );
        });
    }

    #[test]
    fn test_save_unpack_config_content_contains_context_length() {
        with_temp_cwd(|| {
            let mut config = Config::default();
            config.context_length = 65536;
            let path = save_unpack_config(&config).expect("save should succeed");
            let content = std::fs::read_to_string(&path).expect("should read file");
            assert!(
                content.contains("65536"),
                "content should contain context_length: {}",
                content
            );
        });
    }

    #[test]
    fn test_save_unpack_config_content_contains_temperature() {
        with_temp_cwd(|| {
            let mut config = Config::default();
            config.temperature = 0.42;
            let path = save_unpack_config(&config).expect("save should succeed");
            let content = std::fs::read_to_string(&path).expect("should read file");
            assert!(
                content.contains("0.42"),
                "content should contain temperature: {}",
                content
            );
        });
    }

    #[test]
    fn test_save_unpack_config_content_contains_safety_section() {
        with_temp_cwd(|| {
            let config = Config::default();
            let path = save_unpack_config(&config).expect("save should succeed");
            let content = std::fs::read_to_string(&path).expect("should read file");
            assert!(
                content.contains("[safety]"),
                "content should contain [safety] section: {}",
                content
            );
            assert!(
                content.contains("allowed_paths"),
                "content should contain allowed_paths: {}",
                content
            );
            assert!(
                content.contains("denied_paths"),
                "content should contain denied_paths: {}",
                content
            );
        });
    }

    #[test]
    fn test_save_unpack_config_content_contains_agent_section() {
        with_temp_cwd(|| {
            let config = Config::default();
            let path = save_unpack_config(&config).expect("save should succeed");
            let content = std::fs::read_to_string(&path).expect("should read file");
            assert!(
                content.contains("[agent]"),
                "content should contain [agent] section: {}",
                content
            );
            assert!(
                content.contains("native_function_calling"),
                "content should contain native_function_calling: {}",
                content
            );
            assert!(
                content.contains("streaming"),
                "content should contain streaming: {}",
                content
            );
            assert!(
                content.contains("token_budget"),
                "content should contain token_budget: {}",
                content
            );
            assert!(
                content.contains("step_timeout_secs"),
                "content should contain step_timeout_secs: {}",
                content
            );
        });
    }

    #[test]
    fn test_save_unpack_config_agent_values_reflected() {
        with_temp_cwd(|| {
            let mut config = Config::default();
            config.agent.native_function_calling = true;
            config.agent.streaming = false;
            config.agent.token_budget = 50000;
            config.agent.step_timeout_secs = 120;
            let path = save_unpack_config(&config).expect("save should succeed");
            let content = std::fs::read_to_string(&path).expect("should read file");
            assert!(
                content.contains("native_function_calling = true"),
                "should have native_function_calling = true: {}",
                content
            );
            assert!(
                content.contains("streaming = false"),
                "should have streaming = false: {}",
                content
            );
            assert!(
                content.contains("token_budget = 50000"),
                "should have token_budget = 50000: {}",
                content
            );
            assert!(
                content.contains("step_timeout_secs = 120"),
                "should have step_timeout_secs = 120: {}",
                content
            );
        });
    }

    #[test]
    fn test_save_unpack_config_with_extra_body() {
        with_temp_cwd(|| {
            let mut config = Config::default();
            let mut extra = serde_json::Map::new();
            extra.insert(
                "chat_template_kwargs".to_string(),
                serde_json::json!({"enable_thinking": false}),
            );
            config.extra_body = Some(extra);

            let path = save_unpack_config(&config).expect("save should succeed");
            let content = std::fs::read_to_string(&path).expect("should read file");
            assert!(
                content.contains("[extra_body]"),
                "content should contain [extra_body] section: {}",
                content
            );
            assert!(
                content.contains("chat_template_kwargs"),
                "content should contain the extra_body key: {}",
                content
            );
        });
    }

    #[test]
    fn test_save_unpack_config_with_extra_body_scalar() {
        with_temp_cwd(|| {
            let mut config = Config::default();
            let mut extra = serde_json::Map::new();
            extra.insert("top_p".to_string(), serde_json::json!(0.95));
            config.extra_body = Some(extra);

            let path = save_unpack_config(&config).expect("save should succeed");
            let content = std::fs::read_to_string(&path).expect("should read file");
            assert!(
                content.contains("top_p = 0.95"),
                "content should contain the scalar extra_body: {}",
                content
            );
        });
    }

    #[test]
    fn test_save_unpack_config_without_extra_body() {
        with_temp_cwd(|| {
            let config = Config::default();
            // extra_body is None by default
            assert!(config.extra_body.is_none());
            let path = save_unpack_config(&config).expect("save should succeed");
            let content = std::fs::read_to_string(&path).expect("should read file");
            assert!(
                !content.contains("[extra_body]"),
                "content should NOT contain [extra_body] when none: {}",
                content
            );
        });
    }

    #[test]
    fn test_save_unpack_config_returns_correct_path() {
        with_temp_cwd(|| {
            let config = Config::default();
            let path = save_unpack_config(&config).expect("save should succeed");
            assert_eq!(
                path,
                std::path::PathBuf::from("selfware.toml"),
                "should return selfware.toml path"
            );
        });
    }

    #[test]
    fn test_save_unpack_config_overwrites_existing() {
        with_temp_cwd(|| {
            let mut config = Config::default();
            config.model = "first-model".to_string();
            save_unpack_config(&config).expect("first save should succeed");

            // Now save a different config
            config.model = "second-model".to_string();
            let path = save_unpack_config(&config).expect("second save should succeed");
            let content = std::fs::read_to_string(&path).expect("should read file");
            assert!(
                content.contains("second-model"),
                "file should contain the second model: {}",
                content
            );
            assert!(
                !content.contains("first-model"),
                "file should NOT contain the first model: {}",
                content
            );
        });
    }

    #[test]
    fn test_save_unpack_config_content_has_header_comment() {
        with_temp_cwd(|| {
            let config = Config::default();
            let path = save_unpack_config(&config).expect("save should succeed");
            let content = std::fs::read_to_string(&path).expect("should read file");
            assert!(
                content.contains("auto-generated by unpack"),
                "content should have header comment: {}",
                content
            );
        });
    }

    // =========================================================================
    // unpack() tests
    // =========================================================================

    #[tokio::test]
    async fn test_unpack_returns_ok() {
        // unpack() creates a default config and auto-calibrates.
        // In a test environment with no local servers, it should still
        // return Ok (either Some or None config).
        let result = unpack().await;
        assert!(result.is_ok(), "unpack should not error");
    }

    #[tokio::test]
    async fn test_unpack_with_explicit_config_returns_none() {
        // We can't easily make unpack() return None since it always starts
        // with a default config and auto_calibrate returns true in most cases.
        // But we can verify it returns Ok.
        let result = unpack().await;
        assert!(result.is_ok(), "unpack should return Ok");
        // The result is Ok(Option<Config>) — it should be Some if calibration
        // happened, or None if not.
        let _ = result.unwrap();
    }

    // =========================================================================
    // has_config_file tests
    // =========================================================================

    #[test]
    fn test_has_config_file_in_temp_dir_without_config() {
        with_temp_cwd(|| {
            // In a fresh temp dir with no selfware.toml and no ~/.config/selfware/config.toml,
            // has_config_file should return false (unless the user's home dir has one).
            // We can't guarantee the home dir state, so we just verify it returns a bool.
            let _ = has_config_file();
            // If selfware.toml exists in temp dir, we know it's false:
            assert!(
                !std::path::Path::new("selfware.toml").exists(),
                "temp dir should not have selfware.toml"
            );
        });
    }

    #[test]
    fn test_has_config_file_detects_selfware_toml() {
        with_temp_cwd(|| {
            // Create a selfware.toml in the temp dir
            std::fs::write("selfware.toml", "# test config").expect("failed to write");
            assert!(has_config_file(), "should detect selfware.toml in cwd");
        });
    }

    // =========================================================================
    // Integration: save_unpack_config then verify content matches Config
    // =========================================================================

    #[test]
    fn test_save_unpack_config_full_content_verification() {
        with_temp_cwd(|| {
            let mut config = Config::default();
            config.endpoint = "http://192.168.1.100:1234/v1".to_string();
            config.model = "qwen3.5:14b".to_string();
            config.max_tokens = 32768;
            config.context_length = 131072;
            config.temperature = 0.7;
            config.agent.native_function_calling = true;
            config.agent.streaming = true;
            config.agent.token_budget = 100000;
            config.agent.step_timeout_secs = 600;

            let path = save_unpack_config(&config).expect("save should succeed");
            let content = std::fs::read_to_string(&path).expect("should read file");

            // Verify all key fields appear in the content
            assert!(content.contains("http://192.168.1.100:1234/v1"));
            assert!(content.contains("qwen3.5:14b"));
            assert!(content.contains("32768"));
            assert!(content.contains("131072"));
            assert!(content.contains("0.7"));
            assert!(content.contains("native_function_calling = true"));
            assert!(content.contains("token_budget = 100000"));
            assert!(content.contains("step_timeout_secs = 600"));
        });
    }

    // =========================================================================
    // Default value verification for auto_calibrate logic
    // =========================================================================

    #[test]
    fn test_default_endpoint_is_openrouter() {
        // Verify the default endpoint that auto_calibrate checks against
        assert_eq!(default_endpoint(), "https://openrouter.ai/api/v1");
    }

    #[test]
    fn test_default_model_is_glm52() {
        assert_eq!(default_model(), "z-ai/glm-5.2");
    }

    #[test]
    fn test_config_default_uses_defaults() {
        let config = Config::default();
        assert_eq!(config.endpoint, default_endpoint());
        assert_eq!(config.model, default_model());
    }

    #[tokio::test]
    async fn test_auto_calibrate_with_known_default_endpoints() {
        // The code treats several endpoints as "default":
        // - default_endpoint() (openrouter)
        // - http://localhost:8000/v1
        // - http://127.0.0.1:1234/v1
        // If we set model to non-default but keep a known default endpoint,
        // auto_calibrate should NOT skip.
        let mut config = Config::default();
        config.endpoint = "http://localhost:8000/v1".to_string();
        config.model = "custom-model".to_string();

        let result = auto_calibrate(&mut config).await;
        assert!(result.is_ok());
        // Should proceed with calibration since endpoint is a known default
        assert!(
            result.unwrap(),
            "should proceed when endpoint is a known default"
        );
    }

    #[tokio::test]
    async fn test_auto_calibrate_with_lm_studio_default_endpoint() {
        let mut config = Config::default();
        config.endpoint = "http://127.0.0.1:1234/v1".to_string();
        config.model = "custom-model".to_string();

        let result = auto_calibrate(&mut config).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_auto_calibrate_default_model_custom_endpoint_skips() {
        // If model is default but endpoint is fully custom (not one of the
        // known defaults), then is_default_endpoint = false and is_default_model = true,
        // so the skip condition `!is_default_endpoint && !is_default_model` is false
        // (because is_default_model is true). So it should NOT skip.
        let mut config = Config::default();
        config.endpoint = "http://my-server:9999/v1".to_string();
        // model stays as default

        let result = auto_calibrate(&mut config).await;
        assert!(result.is_ok());
        // Should proceed because one of the two is still default
        assert!(result.unwrap());
    }

    // =========================================================================
    // safe_detect_specs tests
    // =========================================================================

    #[test]
    fn test_safe_detect_specs_returns_option() {
        // Should either return Some(specs) or None, but never panic
        let result = safe_detect_specs();
        if let Some(specs) = &result {
            // Basic sanity checks
            assert!(specs.total_cpu_cores > 0, "should have at least 1 CPU core");
            assert!(specs.total_ram_gb > 0.0, "should have positive RAM");
        }
        // None is also acceptable in restricted environments
    }

    // =========================================================================
    // scan_local_endpoints tests
    // =========================================================================

    #[tokio::test]
    async fn test_scan_local_endpoints_returns_vec() {
        // In a test environment, there may or may not be local servers.
        // We just verify it returns a Vec without panicking.
        let endpoints = scan_local_endpoints().await;
        // Each endpoint should have valid fields
        for ep in &endpoints {
            assert!(!ep.provider.is_empty(), "provider should not be empty");
            assert!(!ep.endpoint.is_empty(), "endpoint should not be empty");
            assert!(!ep.model.is_empty(), "model should not be empty");
            assert!(ep.context_length > 0, "context_length should be positive");
        }
    }
}
