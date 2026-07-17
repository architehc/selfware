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

    fn assert_credential_safe(&self) -> Result<()> {
        crate::config::api_key::assert_credential_endpoint_safe(
            &self.endpoint,
            self.api_key.is_some(),
        )
    }

    /// Fetch available models from the /models endpoint.
    pub async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
        self.assert_credential_safe()?;
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
    ///
    /// The returned config is run through the same validation the loader
    /// applies on disk reads ([`Config::validate_generated`]), so callers can
    /// persist it directly — a config selfware would reject is an error here,
    /// not a file on disk.
    pub async fn generate_config(&self, model: &str) -> Result<Config> {
        let models = self.fetch_models().await?;
        let model_info = models.iter().find(|m| m.id == model);
        // Re-run tests to get results (cached in practice by the CLI handler)
        let results = self.run_tests(model).await?;

        // A model missing from /models — or listed WITHOUT a max_model_len
        // (common on hosted gateways like OpenRouter) — yields 0, which the
        // loader rejects ("context_length must be greater than 0"). Treat 0
        // as "unknown" and fall back to the conservative unknown-model context.
        let max_model_len = model_info
            .map(|m| m.max_model_len)
            .filter(|&n| n > 0)
            .unwrap_or(super::UNKNOWN_MODEL_CONTEXT_LENGTH);
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
        // Preferred budget: context minus headroom for output + overhead. When
        // that saturates to 0 (small/unknown context), the 0 sentinel lets
        // `validate_generated` derive the standard 60%-of-context budget.
        config.agent.token_budget = max_model_len.saturating_sub(max_tokens + 50_000);

        if results.thinking_eats_tokens {
            let mut extra = serde_json::Map::new();
            extra.insert(
                "chat_template_kwargs".to_string(),
                json!({"enable_thinking": false}),
            );
            config.extra_body = Some(extra);
        }

        // Never hand back a config the loader would refuse: derive the
        // remaining limits and validate exactly as a disk read would.
        config.validate_generated()?;

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
        self.assert_credential_safe()?;
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

    // ── generate_config: never emit a config the loader rejects ─────────────

    /// Minimal OpenAI-ish HTTP server: serves `/models` reporting the given
    /// `max_model_len` and a canned non-streaming chat completion. Raw TCP in
    /// a background thread — no async runtime needed on the serving side.
    fn spawn_mock_server(max_model_len: u64) -> String {
        use std::io::{BufRead, BufReader, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let mut reader = BufReader::new(stream);
                // Read the request line + headers (body not needed).
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).is_err() {
                    continue;
                }
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            if line == "\r\n" {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                let body = if request_line.contains("/models") {
                    format!(
                        r#"{{"data":[{{"id":"m0","root":"m0","owned_by":"vllm","max_model_len":{}}}]}}"#,
                        max_model_len
                    )
                } else {
                    // chat/completions (streaming probe only checks the status)
                    r#"{"choices":[{"message":{"content":"HELLO_SELFWARE"}}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#.to_string()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let mut stream = reader.into_inner();
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        format!("http://{}/v1", addr)
    }

    #[tokio::test]
    async fn test_generate_config_zero_max_model_len_uses_conservative_context() {
        // A /models listing WITHOUT a usable max_model_len (0) used to produce
        // context_length=0 / token_budget=0 — a config the loader rejects.
        let url = spawn_mock_server(0);
        let configurator = AutoConfigurator::new(&url, None);
        let config = configurator.generate_config("m0").await.unwrap();
        assert_eq!(
            config.context_length,
            crate::config::UNKNOWN_MODEL_CONTEXT_LENGTH,
            "unknown context must fall back to the conservative default, not 0"
        );
        assert!(
            config.agent.token_budget > 0,
            "token_budget must never be 0 in a generated config"
        );
        // And the whole thing passes the same validation a disk read applies.
        config.validate().unwrap();
    }

    #[tokio::test]
    async fn test_generate_config_uses_reported_context_length() {
        let url = spawn_mock_server(131072);
        let configurator = AutoConfigurator::new(&url, None);
        let config = configurator.generate_config("m0").await.unwrap();
        assert_eq!(config.context_length, 131072);
        // context minus output+overhead headroom, unchanged from before.
        assert_eq!(config.agent.token_budget, 131072 - (16384 + 50_000));
        config.validate().unwrap();
    }
}
