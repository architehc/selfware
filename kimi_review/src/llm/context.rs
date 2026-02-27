//! Context window management for LLM conversations

use super::SamplingParams;
use crate::error::LLMError;
use tracing::{debug, info, warn};

/// Context window manager for handling long conversations
pub struct ContextWindowManager {
    max_tokens: usize,
    compression_strategy: CompressionStrategy,
}

/// Compression strategy for context
#[derive(Debug, Clone)]
pub enum CompressionStrategy {
    /// Keep only recent tokens
    SlidingWindow { window_size: usize },
    /// Summarize older content
    Hierarchical { summary_interval: usize },
    /// Keep important messages
    Selective { importance_threshold: f32 },
    /// Hybrid approach
    Hybrid(Box<CompressionStrategy>, Box<CompressionStrategy>),
}

/// Conversation context
#[derive(Debug, Clone, Default)]
pub struct ConversationContext {
    pub messages: Vec<Message>,
    pub token_count: usize,
}

/// Message in conversation
#[derive(Debug, Clone)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    pub token_count: usize,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Message role
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

impl ContextWindowManager {
    /// Create a new context window manager
    pub fn new(max_tokens: usize, strategy: CompressionStrategy) -> Self {
        Self {
            max_tokens,
            compression_strategy: strategy,
        }
    }
    
    /// Get max tokens
    pub fn max_tokens(&self) -> usize {
        self.max_tokens
    }
    
    /// Add a message to context
    pub fn add_message(&self, context: &mut ConversationContext, role: MessageRole, content: impl Into<String>) {
        let content = content.into();
        let token_count = self.estimate_tokens(&content);
        
        context.messages.push(Message {
            role,
            content,
            token_count,
            timestamp: chrono::Utc::now(),
        });
        
        context.token_count += token_count;
    }
    
    /// Compress context if needed
    pub async fn compress_if_needed(&self, context: &mut ConversationContext) -> Result<(), LLMError> {
        if context.token_count <= self.max_tokens * 9 / 10 {
            return Ok(());
        }
        
        info!(current_tokens = context.token_count, max_tokens = self.max_tokens, "Compressing context");
        
        match &self.compression_strategy {
            CompressionStrategy::SlidingWindow { window_size } => {
                self.sliding_window_compress(context, *window_size);
            }
            CompressionStrategy::Hierarchical { summary_interval } => {
                self.hierarchical_compress(context, *summary_interval).await?;
            }
            CompressionStrategy::Selective { importance_threshold } => {
                self.selective_compress(context, *importance_threshold).await?;
            }
            CompressionStrategy::Hybrid(a, b) => {
                self.apply_strategy(context, a).await?;
                if context.token_count > self.max_tokens * 8 / 10 {
                    self.apply_strategy(context, b).await?;
                }
            }
        }
        
        info!(new_tokens = context.token_count, "Context compressed");
        
        Ok(())
    }
    
    /// Sliding window compression
    fn sliding_window_compress(&self, context: &mut ConversationContext, window_size: usize) {
        if context.messages.len() <= window_size {
            return;
        }
        
        let to_remove = context.messages.len() - window_size;
        let removed_tokens: usize = context.messages[..to_remove]
            .iter()
            .map(|m| m.token_count)
            .sum();
        
        context.messages.drain(..to_remove);
        context.token_count -= removed_tokens;
        
        debug!(removed_messages = to_remove, "Sliding window compression applied");
    }
    
    /// Hierarchical compression with summarization
    async fn hierarchical_compress(&self, context: &mut ConversationContext, interval: usize) -> Result<(), LLMError> {
        // Group messages into chunks
        let chunks: Vec<_> = context.messages
            .chunks(interval)
            .map(|c| c.to_vec())
            .collect();
        
        let mut new_messages = Vec::new();
        
        for (i, chunk) in chunks.iter().enumerate() {
            if i == chunks.len() - 1 {
                // Keep most recent chunk intact
                new_messages.extend(chunk.clone());
            } else {
                // Summarize older chunks
                let summary = self.summarize_chunk(chunk).await?;
                new_messages.push(Message {
                    role: MessageRole::System,
                    content: format!("[Summary of {} messages]: {}", chunk.len(), summary),
                    token_count: self.estimate_tokens(&summary) + 20,
                    timestamp: chunk.last().map(|m| m.timestamp).unwrap_or_else(chrono::Utc::now),
                });
            }
        }
        
        context.token_count = new_messages.iter().map(|m| m.token_count).sum();
        context.messages = new_messages;
        
        Ok(())
    }
    
    /// Selective compression keeping important messages
    async fn selective_compress(&self, context: &mut ConversationContext, threshold: f32) -> Result<(), LLMError> {
        // Score each message by importance
        let mut scored: Vec<_> = context.messages
            .iter()
            .map(|m| (m, self.score_importance(m)))
            .collect();
        
        // Sort by importance
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        // Keep messages above threshold
        let to_keep: Vec<_> = scored
            .into_iter()
            .filter(|(_, score)| *score >= threshold)
            .map(|(m, _)| m.clone())
            .collect();
        
        context.token_count = to_keep.iter().map(|m| m.token_count).sum();
        context.messages = to_keep;
        
        Ok(())
    }
    
    /// Apply a compression strategy
    async fn apply_strategy(&self, context: &mut ConversationContext, strategy: &CompressionStrategy) -> Result<(), LLMError> {
        match strategy {
            CompressionStrategy::SlidingWindow { window_size } => {
                self.sliding_window_compress(context, *window_size);
                Ok(())
            }
            CompressionStrategy::Hierarchical { summary_interval } => {
                self.hierarchical_compress(context, *summary_interval).await
            }
            CompressionStrategy::Selective { importance_threshold } => {
                self.selective_compress(context, *importance_threshold).await
            }
            CompressionStrategy::Hybrid(a, b) => {
                self.apply_strategy(context, a).await?;
                self.apply_strategy(context, b).await
            }
        }
    }
    
    /// Summarize a chunk of messages
    async fn summarize_chunk(&self, chunk: &[Message]) -> Result<String, LLMError> {
        // In a real implementation, this would use the LLM to summarize
        // For now, return a placeholder
        Ok(format!("Summary of {} messages", chunk.len()))
    }
    
    /// Score message importance
    fn score_importance(&self, message: &Message) -> f32 {
        // Simple scoring based on role and content
        let role_score = match message.role {
            MessageRole::System => 1.0,
            MessageRole::User => 0.8,
            MessageRole::Assistant => 0.6,
        };
        
        // Longer messages might be more important
        let length_score = (message.content.len() as f32 / 1000.0).min(1.0);
        
        role_score * 0.7 + length_score * 0.3
    }
    
    /// Estimate token count for text
    fn estimate_tokens(&self, text: &str) -> usize {
        // Simple estimation: ~4 characters per token
        // In production, use actual tokenizer
        (text.len() / 4).max(1)
    }
    
    /// Build prompt from context
    pub fn build_prompt(&self, context: &ConversationContext) -> String {
        context.messages
            .iter()
            .map(|m| format!("{:?}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Message {
    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            token_count: 0,
            timestamp: chrono::Utc::now(),
        }
    }
    
    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            token_count: 0,
            timestamp: chrono::Utc::now(),
        }
    }
    
    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            token_count: 0,
            timestamp: chrono::Utc::now(),
        }
    }
}
