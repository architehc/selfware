use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

pub mod types;

use types::*;

pub struct KimiClient {
    client: Client,
    config: crate::config::Config,
    base_url: String,
}

impl KimiClient {
    pub fn new(config: &crate::config::Config) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .context("Failed to build HTTP client")?;
            
        Ok(Self {
            client,
            base_url: config.endpoint.clone(),
            config: config.clone(),
        })
    }

    pub async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
        thinking: ThinkingMode,
    ) -> Result<ChatResponse> {
        let mut body = serde_json::json!({
            "model": self.config.model,
            "messages": messages,
            "temperature": self.config.temperature,
            "max_tokens": self.config.max_tokens,
            "stream": false,
        });

        if let Some(tools) = tools {
            body["tools"] = serde_json::json!(tools);
        }

        if let ThinkingMode::Disabled = thinking {
            body["thinking"] = serde_json::json!({"type": "disabled"});
        }

        debug!("Sending request to {}/chat/completions", self.base_url);
        
        let response = self.client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            let text = response.text().await?;
            anyhow::bail!("API error: {}", text);
        }

        let chat_response: ChatResponse = response.json().await
            .context("Failed to parse response")?;

        Ok(chat_response)
    }
}

#[derive(Clone, Copy)]
pub enum ThinkingMode {
    Enabled,
    #[allow(dead_code)]
    Disabled,
    #[allow(dead_code)]
    Budget(usize),
}
