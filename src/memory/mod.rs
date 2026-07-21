//! Agent Memory Management
//!
//! Tracks token usage and manages the agent's conversation context.
//! Features:
//! - Token estimation for messages
//! - Context window limit awareness
//! - Message summarization for long conversations
//! - Memory statistics for monitoring

use crate::api::types::Message;
use crate::config::Config;
use crate::token_count::estimate_tokens_with_overhead;
use crate::tools::codemap::update_memory_tokens;
use anyhow::Result;
use chrono::Utc;
use std::collections::VecDeque;

/// Maximum number of memory entries before eviction kicks in.
const MAX_MEMORY_ENTRIES: usize = 10_000;

pub struct AgentMemory {
    context_window: usize,
    max_memory_tokens: usize,
    entries: VecDeque<MemoryEntry>,
    /// Running total of `entries[*].token_estimate`. Maintained incrementally so
    /// that token-budget eviction is O(1) instead of O(N²).
    total_tokens: usize,
}

pub struct MemoryEntry {
    pub timestamp: String,
    pub role: String,
    pub content: String,
    pub token_estimate: usize,
    /// Tool calls made by the assistant, if any.  Preserved so that
    /// structured tool-call content is not lost when a message is stored
    /// in memory.
    pub tool_calls: Option<Vec<crate::api::types::ToolCall>>,
    /// Number of image blocks in the original message.  Preserved so
    /// that multimodal content presence is not lost.
    pub image_count: usize,
}

impl MemoryEntry {
    pub fn from_message(msg: &Message) -> Self {
        let token_estimate = estimate_tokens(msg.content.text());
        Self {
            timestamp: Utc::now().to_rfc3339(),
            role: msg.role.clone(),
            content: msg.content.text().to_string(),
            token_estimate,
            tool_calls: msg.tool_calls.clone(),
            image_count: msg.content.image_count(),
        }
    }
}

fn estimate_tokens(content: &str) -> usize {
    estimate_tokens_with_overhead(content, 10)
}

impl AgentMemory {
    pub fn new(config: &Config) -> Result<Self> {
        // Memory must never exceed the real context window.  Derive the cap
        // from `context_length` (the actual context window the model will
        // use), bounded by the resource quota as a secondary safety net.
        let memory_token_cap = config
            .context_length
            .min(config.resources.quotas.max_context_tokens);

        // Reserve 5 % for tool definitions, formatting, and estimation variance.
        let max_memory_tokens = memory_token_cap.saturating_mul(95) / 100;
        Ok(Self {
            context_window: config.context_length,
            max_memory_tokens,
            entries: VecDeque::new(),
            total_tokens: 0,
        })
    }

    /// Add a message to memory.
    ///
    /// Two eviction strategies are applied in order:
    /// 1. **Entry count limit** -- when the number of entries reaches
    ///    `MAX_MEMORY_ENTRIES`, the oldest 25 % of entries are removed.
    /// 2. **Token budget** -- when the total estimated tokens (including the
    ///    new entry) would exceed `MAX_MEMORY_TOKENS`, the oldest entries are
    ///    removed one at a time until the total is under budget.
    pub fn add_message(&mut self, msg: &Message) {
        // Entry count eviction
        if self.entries.len() >= MAX_MEMORY_ENTRIES {
            let remove_count = MAX_MEMORY_ENTRIES / 4;
            for removed in self.entries.drain(..remove_count) {
                self.total_tokens = self.total_tokens.saturating_sub(removed.token_estimate);
            }
        }

        let new_entry = MemoryEntry::from_message(msg);
        let new_tokens = new_entry.token_estimate;

        // Token budget eviction -- evict oldest entries while over budget.
        // Uses the running `total_tokens` counter for O(1) eviction instead of
        // re-summing the entire VecDeque on every pop.
        while self.total_tokens + new_tokens > self.max_memory_tokens && !self.entries.is_empty() {
            if let Some(removed) = self.entries.pop_front() {
                self.total_tokens = self.total_tokens.saturating_sub(removed.token_estimate);
            }
        }

        self.total_tokens = self.total_tokens.saturating_add(new_tokens);
        self.entries.push_back(new_entry);
        update_memory_tokens(self.total_tokens);
    }

    /// Estimate total token usage across all memory entries.
    pub fn total_estimated_tokens(&self) -> usize {
        self.total_tokens
    }

    /// Get total estimated tokens in memory
    pub fn total_tokens(&self) -> usize {
        self.total_estimated_tokens()
    }

    /// Check if memory is approaching the memory token budget.
    pub fn is_near_limit(&self) -> bool {
        self.total_tokens().saturating_mul(100) > self.max_memory_tokens.saturating_mul(85)
    }

    /// Get the context window size
    pub fn context_window(&self) -> usize {
        self.context_window
    }

    /// Get the maximum tokens allowed in memory.
    pub fn max_memory_tokens(&self) -> usize {
        self.max_memory_tokens
    }

    /// Get number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if memory is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
        self.total_tokens = 0;
        update_memory_tokens(0);
    }

    /// Add a raw memory entry from checkpoint data (used during resume).
    ///
    /// Unlike `add_message`, this accepts pre-serialized fields from a
    /// `TaskCheckpoint`'s `memory_entries` and restores them directly without
    /// going through `Message` conversion. This preserves the exact timestamp,
    /// role, content, and token estimate from the checkpoint.
    pub fn add_raw_entry(
        &mut self,
        timestamp: String,
        role: String,
        content: String,
        token_estimate: usize,
    ) {
        // Entry count eviction
        if self.entries.len() >= MAX_MEMORY_ENTRIES {
            let remove_count = MAX_MEMORY_ENTRIES / 4;
            for removed in self.entries.drain(..remove_count) {
                self.total_tokens = self.total_tokens.saturating_sub(removed.token_estimate);
            }
        }

        // Token budget eviction
        while self.total_tokens + token_estimate > self.max_memory_tokens
            && !self.entries.is_empty()
        {
            if let Some(removed) = self.entries.pop_front() {
                self.total_tokens = self.total_tokens.saturating_sub(removed.token_estimate);
            }
        }

        let entry = MemoryEntry {
            timestamp,
            role,
            content,
            token_estimate,
            tool_calls: None,
            image_count: 0,
        };

        self.total_tokens = self.total_tokens.saturating_add(token_estimate);
        self.entries.push_back(entry);
        update_memory_tokens(self.total_tokens);
    }

    /// Set the total token count directly (used during checkpoint resume to
    /// restore the agent's token budget awareness without re-estimating every
    /// entry).
    pub fn set_total_tokens(&mut self, tokens: usize) {
        self.total_tokens = tokens;
        update_memory_tokens(self.total_tokens);
    }

    /// Get recent entries (last n)
    pub fn recent(&self, n: usize) -> Vec<&MemoryEntry> {
        self.entries.iter().rev().take(n).collect()
    }

    /// Get a formatted summary of recent memory
    pub fn summary(&self, n: usize) -> String {
        let recent = self.recent(n);
        recent
            .iter()
            .map(|e| {
                let preview: String = e.content.chars().take(50).collect();
                format!("[{}] {}: {}...", e.timestamp, e.role, preview)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)] // test config builders: default-then-tweak is clearer
#[path = "../../tests/unit/memory/mod_test.rs"]
mod tests;
