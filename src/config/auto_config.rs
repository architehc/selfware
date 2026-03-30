//! Auto-configuration — probes an LLM endpoint and detects backend type,
//! model capabilities, context length, tool calling support, and streaming.
//!
//! Usage via CLI:
//!   selfware auto-config --endpoint http://localhost:8000/v1 --toml
//!   selfware auto-config --endpoint <https://example.com/v1> --save
//!   selfware auto-config --endpoint \<url\> --api-key \<key\> --save

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tracing::debug;

use super::{Config, RedactedString};

/// Detected backend type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendType {
    Vllm,
    Sglang,
    Ollama,
    LlamaServer,
    OpenAI,
    Unknown,
}

impl BackendType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Vllm => "vLLM",
            Self::Sglang => "SGLang",
            Self::Ollama => "Ollama",
            Self::LlamaServer => "llama.cpp",
            Self::OpenAI => "OpenAI",
            Self::Unknown => "Unknown",
        }
    }
}

/// Model info from /models endpoint.
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub root: String,
    pub owned_by: String,
    pub max_model_len: usize,
}

/// Results from capability detection tests.
#[derive(Debug, Clone)]
pub struct DetectionResults {
    pub backend_type: Option<BackendType>,
    pub function_calling: bool,
    pub streaming: bool,
    pub chat_works: bool,
    pub thinking_supported: bool,
    pub thinking_eats_tokens: bool,
}

/// Probes and auto-configures an LLM endpoint.
pub struct AutoConfigurator {
    endpoint: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl AutoConfigurator {
    pub fn new(endpoint: &str, api_key: Option<&str>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            api_key: api_key.map(|s| s.to_string()),
            client,
        }
    }

    fn auth_header(&self) -> Option<(&str, String)> {
        self.api_key
            .as_ref()
            .map(|key| ("Authorization", format!("Bearer {key}")))
    }

    /// Fetch available models from the /models endpoint.
    pub async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
        let url = format!("{}/models", self.endpoint);
        debug!("Fetching models from {url}");

        let mut req = self.client.get(&url);
        if let Some((k, v)) = self.auth_header() {
            req = req.header(k, v);
        }

        let resp = req
            .send()
            .await
            .with_context(|| format!("Failed to connect to {url}"))?;

        let body: Value = resp
            .json()
            .await
            .context("Failed to parse /models response")?;

        let models = body["data"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|m| ModelInfo {
                id: m["id"].as_str().unwrap_or("unknown").to_string(),
                root: m["root"]
                    .as_str()
                    .unwrap_or(m["id"].as_str().unwrap_or("unknown"))
                    .to_string(),
                owned_by: m["owned_by"].as_str().unwrap_or("unknown").to_string(),
                max_model_len: m["max_model_len"].as_u64().unwrap_or(0) as usize,
            })
            .collect();

        Ok(models)
    }

    /// Run capability detection tests against a model.
    pub async fn run_tests(&self, model: &str) -> Result<DetectionResults> {
        let models = self.fetch_models().await?;

        // Validate model name exists in available models
        let model_info = models.iter().find(|m| m.id == model);
        if model_info.is_none() {
            let available: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
            if !available.is_empty() {
                tracing::warn!(
                    "Model '{}' not found in endpoint. Available models: {:?}",
                    model,
                    available
                );
                println!("⚠️  Warning: Model '{}' not found in endpoint", model);
                println!("   Available models: {:?}", available);
                println!("   Continuing with detection, but this may fail...\n");
            }
        }

        let backend_type = model_info.map(|m| detect_backend(&m.owned_by));

        println!("   [1/4] Testing chat completion...");
        let chat_result = self.test_chat(model, None).await;
        let chat_works = chat_result.is_ok();
        println!(
            "         Chat: {}",
            if chat_works { "OK" } else { "FAILED" }
        );

        println!("   [2/4] Testing thinking mode...");
        let (thinking_supported, thinking_eats_tokens) = if chat_works {
            self.test_thinking(model).await
        } else {
            (false, false)
        };
        println!(
            "         Thinking: {}",
            if thinking_supported {
                if thinking_eats_tokens {
                    "supported (consumes all tokens at low max_tokens)"
                } else {
                    "supported (co-exists with content)"
                }
            } else {
                "not detected"
            }
        );

        println!("   [3/4] Testing streaming...");
        let streaming = if chat_works {
            self.test_streaming(model).await
        } else {
            false
        };
        println!(
            "         Streaming: {}",
            if streaming { "OK" } else { "not detected" }
        );

        println!("   [4/4] Testing function calling...");
        let function_calling = if chat_works {
            self.test_function_calling(model).await
        } else {
            false
        };
        println!(
            "         Function calling: {}",
            if function_calling {
                "native (returns tool_calls)"
            } else {
                "text-based (tools in system prompt)"
            }
        );

        Ok(DetectionResults {
            backend_type,
            function_calling,
            streaming,
            chat_works,
            thinking_supported,
            thinking_eats_tokens,
        })
    }

    /// Generate a Config based on model info and pre-computed detection results.
    pub async fn generate_config(&self, model: &str) -> Result<Config> {
        let models = self.fetch_models().await?;
        let model_info = models.iter().find(|m| m.id == model);
        // Re-run tests to get results (cached in practice by the CLI handler)
        let results = self.run_tests(model).await?;

        let max_model_len = model_info.map(|m| m.max_model_len).unwrap_or(131072);
        let model_root = model_info.map(|m| m.root.as_str()).unwrap_or(model);

        let max_tokens = if results.thinking_eats_tokens {
            16384
        } else if max_model_len > 500_000 {
            32768
        } else if max_model_len > 100_000 {
            16384
        } else {
            8192
        };

        let temperature = if model_root.to_lowercase().contains("qwen") {
            0.6
        } else {
            0.7
        };

        let mut config = Config {
            endpoint: self.endpoint.clone(),
            model: model.to_string(),
            max_tokens,
            context_length: max_model_len,
            temperature,
            ..Default::default()
        };

        if let Some(ref key) = self.api_key {
            config.api_key = Some(RedactedString::new(key));
        }

        config.agent.native_function_calling = results.function_calling;
        config.agent.streaming = results.streaming;
        config.agent.token_budget = max_model_len.saturating_sub(max_tokens + 50_000);

        if results.thinking_eats_tokens {
            let mut extra = serde_json::Map::new();
            extra.insert(
                "chat_template_kwargs".to_string(),
                json!({"enable_thinking": false}),
            );
            config.extra_body = Some(extra);
        }

        Ok(config)
    }

    /// Print a TOML representation of the config.
    pub fn print_config_toml(&self, config: &Config) {
        println!("\n# --- Generated configuration ---");
        println!("endpoint = \"{}\"", config.endpoint);
        println!("model = \"{}\"", config.model);
        println!("max_tokens = {}", config.max_tokens);
        println!("context_length = {}", config.context_length);
        println!("temperature = {}", config.temperature);
        println!();
        println!("[safety]");
        println!("allowed_paths = [\"./**\", \"/tmp/**\"]");
        println!("denied_paths = [\"**/.env\", \"**/secrets/**\", \"**/.ssh/**\"]");
        println!("protected_branches = [\"main\"]");
        println!();
        println!("[agent]");
        println!(
            "native_function_calling = {}",
            config.agent.native_function_calling
        );
        println!("streaming = {}", config.agent.streaming);
        println!("token_budget = {}", config.agent.token_budget);
        println!("step_timeout_secs = {}", config.agent.step_timeout_secs);
        println!();
        println!("[continuous_work]");
        println!("enabled = true");
        println!("checkpoint_interval_tools = 10");
        println!("checkpoint_interval_secs = 300");
        println!("auto_recovery = true");
        println!("max_recovery_attempts = 3");
        if let Some(ref extra) = config.extra_body {
            println!();
            println!("[extra_body]");
            for (k, v) in extra {
                if let Some(obj) = v.as_object() {
                    let inner: Vec<String> =
                        obj.iter().map(|(ik, iv)| format!("{ik} = {iv}")).collect();
                    println!("{k} = {{ {} }}", inner.join(", "));
                } else {
                    println!("{k} = {v}");
                }
            }
        }
        println!();
        println!("[retry]");
        println!("max_retries = 5");
        println!("base_delay_ms = 1000");
        println!("max_delay_ms = 60000");
        println!("# --- end ---\n");
    }

    async fn test_chat(&self, model: &str, extra_body: Option<Value>) -> Result<Value> {
        let url = format!("{}/chat/completions", self.endpoint);
        let mut body = json!({
            "model": model,
            "messages": [{"role": "user", "content": "Say exactly: HELLO_SELFWARE"}],
            "max_tokens": 64,
            "temperature": 0.0,
            "stream": false,
        });

        if let Some(extra) = extra_body {
            if let (Some(b), Some(e)) = (body.as_object_mut(), extra.as_object()) {
                for (k, v) in e {
                    b.insert(k.clone(), v.clone());
                }
            }
        }

        let mut req = self.client.post(&url).json(&body);
        if let Some((k, v)) = self.auth_header() {
            req = req.header(k, v);
        }

        let resp = req.send().await?;
        let result: Value = resp.json().await?;
        Ok(result)
    }

    async fn test_thinking(&self, model: &str) -> (bool, bool) {
        let result = match self.test_chat(model, None).await {
            Ok(r) => r,
            Err(_) => return (false, false),
        };

        let content = result["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("");
        let has_reasoning = result["choices"][0]["message"]["reasoning_content"]
            .as_str()
            .or_else(|| result["choices"][0]["message"]["reasoning"].as_str())
            .is_some();

        let content_empty = content.is_empty() || content == "null";
        (has_reasoning, has_reasoning && content_empty)
    }

    async fn test_streaming(&self, model: &str) -> bool {
        let url = format!("{}/chat/completions", self.endpoint);
        let body = json!({
            "model": model,
            "messages": [{"role": "user", "content": "Hi"}],
            "max_tokens": 16,
            "temperature": 0.0,
            "stream": true,
        });

        let mut req = self.client.post(&url).json(&body);
        if let Some((k, v)) = self.auth_header() {
            req = req.header(k, v);
        }

        match req.send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    async fn test_function_calling(&self, model: &str) -> bool {
        let url = format!("{}/chat/completions", self.endpoint);
        let body = json!({
            "model": model,
            "messages": [{"role": "user", "content": "What time is it in Tokyo?"}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_time",
                    "description": "Get the current time in a city",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "city": {"type": "string", "description": "City name"}
                        },
                        "required": ["city"]
                    }
                }
            }],
            "tool_choice": "auto",
            "max_tokens": 256,
            "temperature": 0.0,
            "stream": false,
            "chat_template_kwargs": {"enable_thinking": false},
        });

        let mut req = self.client.post(&url).json(&body);
        if let Some((k, v)) = self.auth_header() {
            req = req.header(k, v);
        }

        match req.send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    return false;
                }
                let result: Value = match resp.json().await {
                    Ok(v) => v,
                    Err(_) => return false,
                };
                result["choices"][0]["message"]["tool_calls"]
                    .as_array()
                    .map(|a| !a.is_empty())
                    .unwrap_or(false)
            }
            Err(_) => false,
        }
    }
}

fn detect_backend(owned_by: &str) -> BackendType {
    let lower = owned_by.to_lowercase();
    if lower.contains("vllm") {
        BackendType::Vllm
    } else if lower.contains("sglang") {
        BackendType::Sglang
    } else if lower.contains("ollama") {
        BackendType::Ollama
    } else if lower.contains("llama") {
        BackendType::LlamaServer
    } else if lower.contains("openai") {
        BackendType::OpenAI
    } else {
        BackendType::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_backend() {
        assert_eq!(detect_backend("vllm"), BackendType::Vllm);
        assert_eq!(detect_backend("sglang"), BackendType::Sglang);
        assert_eq!(detect_backend("ollama"), BackendType::Ollama);
        assert_eq!(detect_backend("openai"), BackendType::OpenAI);
        assert_eq!(detect_backend("random"), BackendType::Unknown);
    }

    #[test]
    fn test_backend_name() {
        assert_eq!(BackendType::Vllm.name(), "vLLM");
        assert_eq!(BackendType::Sglang.name(), "SGLang");
    }

    #[test]
    fn test_configurator_new() {
        let c = AutoConfigurator::new("http://localhost:8000/v1", Some("test-key"));
        assert_eq!(c.endpoint, "http://localhost:8000/v1");
        assert_eq!(c.api_key.as_deref(), Some("test-key"));
    }

    #[test]
    fn test_configurator_strips_trailing_slash() {
        let c = AutoConfigurator::new("http://localhost:8000/v1/", None);
        assert_eq!(c.endpoint, "http://localhost:8000/v1");
    }
}
