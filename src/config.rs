use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    pub api_key: Option<String>,
    
    #[serde(default)]
    pub safety: SafetyConfig,
    
    #[serde(default)]
    pub agent: AgentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfig {
    #[serde(default = "default_allowed_paths")]
    pub allowed_paths: Vec<String>,
    #[serde(default)]
    pub denied_paths: Vec<String>,
    #[serde(default = "default_protected_branches")]
    pub protected_branches: Vec<String>,
    #[serde(default = "default_require_confirmation")]
    pub require_confirmation: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
    #[serde(default = "default_step_timeout")]
    pub step_timeout_secs: u64,
    #[serde(default = "default_token_budget")]
    pub token_budget: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            endpoint: default_endpoint(),
            model: default_model(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            api_key: None,
            safety: SafetyConfig::default(),
            agent: AgentConfig::default(),
        }
    }
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            allowed_paths: default_allowed_paths(),
            denied_paths: vec![],
            protected_branches: default_protected_branches(),
            require_confirmation: default_require_confirmation(),
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: default_max_iterations(),
            step_timeout_secs: default_step_timeout(),
            token_budget: default_token_budget(),
        }
    }
}

fn default_endpoint() -> String { "http://localhost:8888/v1".to_string() }
fn default_model() -> String { "kimi-k2.5".to_string() }
fn default_max_tokens() -> usize { 32768 }
fn default_temperature() -> f32 { 1.0 }
fn default_max_iterations() -> usize { 100 }
fn default_step_timeout() -> u64 { 300 }
fn default_token_budget() -> usize { 500000 }
fn default_allowed_paths() -> Vec<String> { vec!["./**".to_string()] }
fn default_protected_branches() -> Vec<String> { vec!["main".to_string(), "master".to_string()] }
fn default_require_confirmation() -> Vec<String> { 
    vec!["git_push".to_string(), "file_delete".to_string(), "shell_exec".to_string()] 
}

impl Config {
    pub fn load(path: Option<&str>) -> Result<Self> {
        match path {
            Some(p) => {
                let content = std::fs::read_to_string(p)
                    .with_context(|| format!("Failed to read config from {}", p))?;
                toml::from_str(&content).context("Failed to parse config")
            }
            None => {
                // Try default locations
                let default_paths = ["kimi-agent.toml", "~/.config/kimi-agent/config.toml"];
                for p in &default_paths {
                    if let Ok(content) = std::fs::read_to_string(p) {
                        return toml::from_str(&content).context("Failed to parse config");
                    }
                }
                eprintln!("No config file found, using defaults");
                Ok(Self::default())
            }
        }
    }
}
