//! LLM inference management for vLLM and Ollama

use crate::config::LLMConfig;
use crate::error::{LLMError, SelfwareError};
use crate::resource::gpu::QuantizationLevel;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

pub mod context;
pub mod engine;
pub mod model_manager;
pub mod queue;
pub mod tokenizer;
pub mod vllm;

pub use context::ContextWindowManager;
pub use engine::{InferenceEngine, GenerationParams, TokenOutput};
pub use model_manager::{ModelInstance, ModelLifecycleManager, ModelState};
pub use queue::{InferenceQueue, InferenceRequest};
pub use tokenizer::Tokenizer;

/// LLM provider types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LLMProvider {
    VLLM,
    Ollama,
}

impl std::str::FromStr for LLMProvider {
    type Err = SelfwareError;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "vllm" => Ok(Self::VLLM),
            "ollama" => Ok(Self::Ollama),
            _ => Err(SelfwareError::Config(format!("Unknown LLM provider: {}", s))),
        }
    }
}

/// Model configuration for loading
#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub model_path: std::path::PathBuf,
    pub quantization: QuantizationLevel,
    pub tensor_parallel_size: usize,
    pub gpu_memory_utilization: f32,
    pub max_model_len: usize,
    pub enable_prefix_caching: bool,
    pub enable_chunked_prefill: bool,
    pub max_num_seqs: usize,
    pub critical: bool,
}

impl From<&crate::config::LLMConfig> for ModelConfig {
    fn from(config: &crate::config::LLMConfig) -> Self {
        Self {
            model_path: config.model_path.clone(),
            quantization: QuantizationLevel::None, // Will be adjusted
            tensor_parallel_size: config.tensor_parallel_size,
            gpu_memory_utilization: config.gpu_memory_utilization,
            max_model_len: config.max_model_len,
            enable_prefix_caching: config.enable_prefix_caching,
            enable_chunked_prefill: config.enable_chunked_prefill,
            max_num_seqs: config.max_num_seqs,
            critical: true,
        }
    }
}

/// Sampling parameters for generation
#[derive(Debug, Clone)]
pub struct SamplingParams {
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: i32,
    pub max_tokens: usize,
    pub stop_sequences: Vec<String>,
    pub presence_penalty: f32,
    pub frequency_penalty: f32,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_p: 0.9,
            top_k: -1,
            max_tokens: 1024,
            stop_sequences: Vec::new(),
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        }
    }
}

/// Request output from LLM
#[derive(Debug, Clone)]
pub struct RequestOutput {
    pub text: String,
    pub tokens: Vec<u32>,
    pub finish_reason: Option<FinishReason>,
    pub usage: TokenUsage,
}

/// Finish reason for generation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
}

/// Token usage statistics
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// LLM engine trait
#[async_trait::async_trait]
pub trait LLMEngine: Send + Sync {
    /// Generate text from prompt
    async fn generate(
        &self,
        prompt: &str,
        params: SamplingParams,
    ) -> Result<RequestOutput, LLMError>;
    
    /// Generate text with streaming
    async fn generate_stream(
        &self,
        prompt: &str,
        params: SamplingParams,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<TokenOutput, LLMError>>, LLMError>;
    
    /// Get model info
    async fn model_info(&self) -> Result<ModelInfo, LLMError>;
    
    /// Health check
    async fn health(&self) -> Result<(), LLMError>;
}

/// Model information
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub max_context_length: usize,
    pub vocab_size: usize,
    pub quantization: Option<String>,
}

/// Token output for streaming
#[derive(Debug, Clone)]
pub struct TokenOutput {
    pub token_id: u32,
    pub text: String,
    pub finish_reason: Option<FinishReason>,
}

/// LLM client for making requests
pub struct LLMClient {
    engine: Arc<dyn LLMEngine>,
    queue: Arc<RwLock<InferenceQueue>>,
    context_manager: Arc<ContextWindowManager>,
}

impl LLMClient {
    /// Create a new LLM client
    pub fn new(
        engine: Arc<dyn LLMEngine>,
        queue: Arc<RwLock<InferenceQueue>>,
        context_manager: Arc<ContextWindowManager>,
    ) -> Self {
        Self {
            engine,
            queue,
            context_manager,
        }
    }
    
    /// Generate text with automatic context management
    pub async fn generate(
        &self,
        prompt: &str,
        params: SamplingParams,
    ) -> Result<RequestOutput, LLMError> {
        // Check context window
        let prompt_tokens = self.estimate_tokens(prompt).await;
        
        if prompt_tokens > self.context_manager.max_tokens() {
            // Compress context
            warn!(tokens = prompt_tokens, "Prompt exceeds context window, compressing");
            // Would compress context here
        }
        
        // Submit to queue
        let request = InferenceRequest::new(
            prompt.to_string(),
            params,
            crate::Priority::Normal,
        );
        
        let rx = self.submit_request(request).await?;
        
        // Wait for result
        // In a real implementation, this would wait for the queue to process
        self.engine.generate(prompt, params).await
    }
    
    /// Submit a request to the queue
    async fn submit_request(
        &self,
        request: InferenceRequest,
    ) -> Result<tokio::sync::oneshot::Receiver<RequestOutput>, LLMError> {
        let mut queue = self.queue.write().await;
        queue.enqueue(request);
        
        // Create channel for result
        let (tx, rx) = tokio::sync::oneshot::channel();
        
        // Store sender for when request is processed
        // In real implementation, this would be stored with the queued request
        
        Ok(rx)
    }
    
    /// Estimate token count for text
    async fn estimate_tokens(&self, text: &str) -> usize {
        // Simple estimation: ~4 characters per token
        // In production, use actual tokenizer
        text.len() / 4
    }
}
