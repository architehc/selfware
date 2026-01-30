use crate::api::KimiClient;
use crate::api::types::Message;
use crate::api::ThinkingMode;
use anyhow::Result;
use tracing::{info, warn, debug};

pub struct ContextCompressor {
    #[allow(dead_code)]
    token_budget: usize,
    compression_threshold: usize,
    min_messages_to_keep: usize,
}


impl ContextCompressor {
    pub fn new(token_budget: usize) -> Self {
        Self {
            token_budget,
            compression_threshold: (token_budget as f32 * 0.85) as usize,
            min_messages_to_keep: 6,
        }
    }
    
    pub fn should_compress(&self, messages: &[Message]) -> bool {
        let estimated = self.estimate_tokens(messages);
        debug!("Estimated tokens: {}/{}", estimated, self.compression_threshold);
        estimated > self.compression_threshold
    }
    
    pub fn estimate_tokens(&self, messages: &[Message]) -> usize {
        messages.iter().map(|m| {
            let chars = m.content.len();
            let factor = if m.content.contains('{') || m.content.contains(';') {
                3
            } else {
                4
            };
            chars / factor + 50
        }).sum()
    }
    
    pub async fn compress(&self, client: &KimiClient, messages: &[Message]) -> Result<Vec<Message>> {
        if messages.len() <= self.min_messages_to_keep + 1 {
            warn!("Too few messages to compress, returning as-is");
            return Ok(messages.to_vec());
        }
        
        info!("Compressing context: {} messages", messages.len());
        
        let system_msg = messages.first().cloned();
        let recent_start = messages.len().saturating_sub(self.min_messages_to_keep);
        let recent_msgs: Vec<Message> = messages[recent_start..].to_vec();
        let to_summarize = &messages[1..recent_start];
        
        if to_summarize.is_empty() {
            return Ok(messages.to_vec());
        }
        
        let summary_content = format!(
            "Summarize these previous interactions concisely. Preserve key facts, decisions, and file paths. Omit routine tool outputs unless they indicate errors.\n\n{}",
            to_summarize.iter().enumerate().map(|(i, m)| {
                let content = if m.content.len() > 500 {
                    format!("{}...[truncated]", &m.content[..500])
                } else {
                    m.content.clone()
                };
                format!("[{}] {}: {}", i, m.role, content)
            }).collect::<Vec<_>>().join("\n\n")
        );
        
        let summary_request = vec![
            Message::system("You are a context summarizer. Compress conversation history while preserving critical information for task completion."),
            Message::user(summary_content)
        ];
        
        let response = client.chat(
            summary_request,
            None,
            ThinkingMode::Disabled,
        ).await?;
        
        let summary = response.choices[0].message.content.clone();
        info!("Generated summary: {} chars", summary.len());
        
        let mut compressed = Vec::new();
        if let Some(sys) = system_msg {
            compressed.push(sys);
        }
        
        compressed.push(Message::user(format!(
            "[CONTEXT SUMMARY - {} earlier messages compressed]:\n{}",
            to_summarize.len(),
            summary
        )));
        
        compressed.push(Message::user("[RECENT CONTEXT]:"));
        compressed.extend(recent_msgs.into_iter().rev());
        
        let new_estimate = self.estimate_tokens(&compressed);
        info!("Compression complete: {} -> {} messages, ~{} tokens", 
              messages.len(), compressed.len(), new_estimate);
        
        Ok(compressed)
    }
    
    pub fn hard_compress(&self, messages: &[Message]) -> Vec<Message> {
        let mut result = Vec::new();
        if let Some(first) = messages.first() {
            result.push(first.clone());
        }
        
        let start = messages.len().saturating_sub(4);
        result.extend(messages[start..].iter().cloned());
        
        result.push(Message::user("[NOTE: Earlier context removed due to token limits. Use file_read if you need to review previous changes.]"));
        
        result
    }
}
