//! LLM engine trait and implementations

use super::{FinishReason, LLMEngine, LLMError, RequestOutput, SamplingParams, TokenOutput, TokenUsage};

/// Base engine implementation
pub struct BaseEngine;

#[async_trait::async_trait]
impl LLMEngine for BaseEngine {
    async fn generate(
        &self,
        _prompt: &str,
        _params: SamplingParams,
    ) -> Result<RequestOutput, LLMError> {
        Err(LLMError::InferenceFailed("Not implemented".to_string()))
    }
    
    async fn generate_stream(
        &self,
        _prompt: &str,
        _params: SamplingParams,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<TokenOutput, LLMError>>, LLMError> {
        Err(LLMError::InferenceFailed("Not implemented".to_string()))
    }
    
    async fn model_info(&self) -> Result<super::ModelInfo, LLMError> {
        Err(LLMError::ModelNotFound("Not implemented".to_string()))
    }
    
    async fn health(&self) -> Result<(), LLMError> {
        Ok(())
    }
}

/// Generation parameters builder
#[derive(Debug, Clone)]
pub struct GenerationParams {
    pub temperature: f32,
    pub top_p: f32,
    pub max_tokens: usize,
    pub stop_sequences: Vec<String>,
    pub enable_caching: bool,
}

impl Default for GenerationParams {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_p: 0.9,
            max_tokens: 1024,
            stop_sequences: Vec::new(),
            enable_caching: true,
        }
    }
}

impl GenerationParams {
    /// Set temperature
    pub fn temperature(mut self, temp: f32) -> Self {
        self.temperature = temp.clamp(0.0, 2.0);
        self
    }
    
    /// Set top_p
    pub fn top_p(mut self, top_p: f32) -> Self {
        self.top_p = top_p.clamp(0.0, 1.0);
        self
    }
    
    /// Set max tokens
    pub fn max_tokens(mut self, max: usize) -> Self {
        self.max_tokens = max;
        self
    }
    
    /// Add stop sequence
    pub fn stop_sequence(mut self, seq: impl Into<String>) -> Self {
        self.stop_sequences.push(seq.into());
        self
    }
    
    /// Enable/disable caching
    pub fn enable_caching(mut self, enable: bool) -> Self {
        self.enable_caching = enable;
        self
    }
    
    /// Convert to sampling params
    pub fn to_sampling_params(&self) -> SamplingParams {
        SamplingParams {
            temperature: self.temperature,
            top_p: self.top_p,
            top_k: -1,
            max_tokens: self.max_tokens,
            stop_sequences: self.stop_sequences.clone(),
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        }
    }
}
