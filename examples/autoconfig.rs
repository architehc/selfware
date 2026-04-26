//! Auto-configurator — probe an LLM endpoint and auto-configure selfware.
//!
//! Given a URL, detects: backend (vLLM/sglang/ollama), model, context length,
//! tool calling support, thinking mode, and generates the correct selfware.toml.
//!
//! Usage:
//!   cargo run --example autoconfig -- http://localhost:8000/v1
//!   cargo run --example autoconfig -- http://127.0.0.1:8000/v1

use anyhow::{Context, Result};
use serde_json::{json, Value};

#[derive(Debug)]
struct EndpointInfo {
    endpoint: String,
    backend: String, // "vllm", "sglang", "ollama", "openai", "unknown"
    model_id: String,
    model_root: String,
    max_model_len: usize,
    #[allow(dead_code)]
    owned_by: String,
    #[allow(dead_code)]
    supports_tools: bool,
    native_tool_calling: bool,
    supports_thinking: bool,
    thinking_uses_all_tokens: bool,
    recommended_max_tokens: usize,
    recommended_temperature: f32,
}

async fn probe_models(base_url: &str) -> Result<Value> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let resp = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("Failed to connect to {url}"))?;

    let body: Value = resp
        .json()
        .await
        .context("Failed to parse /models response")?;

    Ok(body)
}

async fn test_chat(base_url: &str, model: &str, extra_body: Option<Value>) -> Result<Value> {
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let mut body = json!({
        "model": model,
        "messages": [{"role": "user", "content": "Say exactly: HELLO_SELFWARE"}],
        "max_tokens": 64,
        "temperature": 0.0,
        "stream": false,
    });

    if let Some(extra) = extra_body {
        if let (Some(body_obj), Some(extra_obj)) = (body.as_object_mut(), extra.as_object()) {
            for (k, v) in extra_obj {
                body_obj.insert(k.clone(), v.clone());
            }
        }
    }

    let resp = client.post(&url).json(&body).send().await?;
    let result: Value = resp.json().await?;
    Ok(result)
}

async fn test_tool_calling(base_url: &str, model: &str) -> Result<(bool, bool)> {
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let body = json!({
        "model": model,
        "messages": [{"role": "user", "content": "What is the weather in Paris?"}],
        "tools": [{
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Get the weather for a city",
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
    });

    let resp = client.post(&url).json(&body).send().await;
    match resp {
        Ok(r) => {
            let status = r.status();
            let result: Value = r.json().await.unwrap_or(json!({}));

            if !status.is_success() {
                // tool_choice rejected — backend doesn't support native FC
                return Ok((false, false));
            }

            // Check if response has tool_calls
            let has_tool_calls = result["choices"][0]["message"]["tool_calls"]
                .as_array()
                .map(|a| !a.is_empty())
                .unwrap_or(false);

            Ok((true, has_tool_calls))
        }
        Err(_) => Ok((false, false)),
    }
}

async fn probe_endpoint(base_url: &str) -> Result<EndpointInfo> {
    eprintln!("Probing {base_url}...\n");

    // 1. Get model info
    eprintln!("  [1/5] Fetching /models...");
    let models = probe_models(base_url).await?;
    let model_data = &models["data"][0];

    let model_id = model_data["id"].as_str().unwrap_or("unknown").to_string();
    let model_root = model_data["root"].as_str().unwrap_or(&model_id).to_string();
    let max_model_len = model_data["max_model_len"].as_u64().unwrap_or(0) as usize;
    let owned_by = model_data["owned_by"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    let backend = match owned_by.as_str() {
        "vllm" => "vllm",
        "sglang" => "sglang",
        _ if owned_by.contains("ollama") => "ollama",
        _ => "unknown",
    }
    .to_string();

    eprintln!("    Model: {model_id}");
    eprintln!("    Root: {model_root}");
    eprintln!("    Backend: {backend}");
    eprintln!("    Max context: {max_model_len}");

    // 2. Test basic chat (with thinking)
    eprintln!("  [2/5] Testing chat (thinking mode)...");
    let think_result = test_chat(base_url, &model_id, None).await?;
    let content = think_result["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("");
    let reasoning = think_result["choices"][0]["message"]["reasoning_content"]
        .as_str()
        .or_else(|| think_result["choices"][0]["message"]["reasoning"].as_str());
    let has_reasoning = reasoning.is_some();
    let content_empty = content.is_empty() || content == "null";
    let thinking_uses_all = has_reasoning && content_empty;

    eprintln!("    Has reasoning: {has_reasoning}");
    eprintln!("    Content empty (thinking ate tokens): {thinking_uses_all}");
    if !content_empty {
        eprintln!("    Content: {}", &content[..content.len().min(80)]);
    }

    // 3. Test chat without thinking
    eprintln!("  [3/5] Testing chat (no-thinking mode)...");
    let no_think_result = test_chat(
        base_url,
        &model_id,
        Some(json!({"chat_template_kwargs": {"enable_thinking": false}})),
    )
    .await?;
    let no_think_content = no_think_result["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("");
    let no_think_works = !no_think_content.is_empty() && no_think_content.contains("HELLO");

    eprintln!(
        "    No-thinking response: {}",
        &no_think_content[..no_think_content.len().min(80)]
    );
    eprintln!("    Works correctly: {no_think_works}");

    // 4. Test native tool calling
    eprintln!("  [4/5] Testing native tool calling...");
    let (tools_accepted, native_fc_works) = test_tool_calling(base_url, &model_id).await?;

    eprintln!("    Tools API accepted: {tools_accepted}");
    eprintln!("    Native FC returns tool_calls: {native_fc_works}");

    // 5. Determine recommendations
    eprintln!("  [5/5] Computing recommendations...\n");

    let supports_thinking = has_reasoning;
    let native_tool_calling = native_fc_works;

    // Max tokens: for thinking models, reserve room for thinking
    let recommended_max_tokens = if supports_thinking && !thinking_uses_all {
        // Model can think and still output — use generous budget
        (max_model_len / 8).min(32768)
    } else if thinking_uses_all {
        // Thinking eats all tokens with small max_tokens — need larger budget
        // or disable thinking for tool use
        16384
    } else {
        8192
    };

    let recommended_temperature = if model_root.to_lowercase().contains("qwen") {
        0.6 // Qwen recommended for coding tasks
    } else {
        0.7
    };

    let info = EndpointInfo {
        endpoint: base_url.to_string(),
        backend,
        model_id,
        model_root,
        max_model_len,
        owned_by,
        supports_tools: tools_accepted,
        native_tool_calling,
        supports_thinking,
        thinking_uses_all_tokens: thinking_uses_all,
        recommended_max_tokens,
        recommended_temperature,
    };

    Ok(info)
}

fn generate_toml(info: &EndpointInfo) -> String {
    let mut toml = String::new();

    toml.push_str(&format!(
        "# Auto-configured for {} ({}) on {}\n",
        info.model_id, info.backend, info.endpoint
    ));
    toml.push_str(&format!("endpoint = \"{}\"\n", info.endpoint));
    toml.push_str(&format!("model = \"{}\"\n", info.model_id));
    toml.push_str(&format!("max_tokens = {}\n", info.recommended_max_tokens));
    toml.push_str(&format!("context_length = {}\n", info.max_model_len));
    toml.push_str(&format!("temperature = {}\n", info.recommended_temperature));
    toml.push('\n');

    toml.push_str("[safety]\n");
    toml.push_str("allowed_paths = [\"./**\", \"/tmp/**\"]\n");
    toml.push_str("denied_paths = [\"**/.env\", \"**/secrets/**\", \"**/.ssh/**\"]\n");
    toml.push_str("protected_branches = [\"main\"]\n\n");

    toml.push_str("[agent]\n");
    toml.push_str("max_iterations = 50\n");
    toml.push_str("step_timeout_secs = 300\n");
    toml.push_str(&format!(
        "native_function_calling = {}\n",
        info.native_tool_calling
    ));
    toml.push('\n');

    toml.push_str("[continuous_work]\n");
    toml.push_str("enabled = true\n");
    toml.push_str("checkpoint_interval_tools = 10\n");
    toml.push_str("checkpoint_interval_secs = 300\n");
    toml.push_str("auto_recovery = true\n");
    toml.push_str("max_recovery_attempts = 3\n\n");

    // Extra body for thinking mode control
    if info.supports_thinking && info.thinking_uses_all_tokens {
        toml.push_str("# Thinking mode disabled — model consumes all tokens on reasoning\n");
        toml.push_str("# with small max_tokens, leaving content empty.\n");
        toml.push_str("[extra_body]\n");
        toml.push_str("chat_template_kwargs = { enable_thinking = false }\n\n");
    } else if info.supports_thinking {
        toml.push_str("# Thinking mode available — leave enabled for better reasoning\n");
        toml.push_str("# Set enable_thinking = false if responses are empty\n");
        toml.push_str("# [extra_body]\n");
        toml.push_str("# chat_template_kwargs = { enable_thinking = true }\n\n");
    }

    toml.push_str("[retry]\n");
    toml.push_str("max_retries = 5\n");
    toml.push_str("base_delay_ms = 1000\n");
    toml.push_str("max_delay_ms = 60000\n");

    toml
}

#[tokio::main]
async fn main() -> Result<()> {
    let urls: Vec<String> = std::env::args().skip(1).collect();

    if urls.is_empty() {
        eprintln!("Usage: autoconfig <endpoint_url> [endpoint_url2] ...");
        eprintln!("Example: autoconfig http://localhost:8000/v1 http://127.0.0.1:8000/v1");
        return Ok(());
    }

    for url in &urls {
        match probe_endpoint(url).await {
            Ok(info) => {
                eprintln!("=== Configuration for {} ===\n", info.endpoint);
                eprintln!("Backend:            {}", info.backend);
                eprintln!("Model:              {}", info.model_id);
                eprintln!("Root:               {}", info.model_root);
                eprintln!("Max context:        {} tokens", info.max_model_len);
                eprintln!(
                    "Thinking:           {}",
                    if info.supports_thinking { "yes" } else { "no" }
                );
                eprintln!("Thinking eats all:  {}", info.thinking_uses_all_tokens);
                eprintln!("Native FC:          {}", info.native_tool_calling);
                eprintln!("Recommended tokens: {}", info.recommended_max_tokens);
                eprintln!("Recommended temp:   {}", info.recommended_temperature);

                let toml = generate_toml(&info);
                let filename = format!(
                    "selfware-auto-{}.toml",
                    info.model_id.replace(['/', '.'], "-")
                );

                eprintln!("\n--- Generated: {filename} ---\n");
                println!("{toml}");

                std::fs::write(&filename, &toml)?;
                eprintln!("Written to {filename}\n");
            }
            Err(e) => {
                eprintln!("Failed to probe {url}: {e}\n");
            }
        }
    }

    Ok(())
}
