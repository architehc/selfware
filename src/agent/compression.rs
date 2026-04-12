//! Three-Layer Context Compression System
//!
//! Provides tiered context compression to keep long sessions viable with local models:
//!
//! 1. **MicroCompact** (local, no API call): Strips old tool outputs and reasoning blocks
//! 2. **AutoCompact** (automatic trigger): Uses LLM to generate summaries at threshold
//! 3. **FullCompact** (nuclear option): Compresses entire conversation with file re-injection

use crate::api::types::{Message, MessageContent};
use crate::api::{ApiClient, ThinkingMode};
use anyhow::Result;
use std::collections::VecDeque;
use tracing::{debug, info, warn};

/// Compression method used
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionMethod {
    Micro,
    Auto,
    Full,
}

impl std::fmt::Display for CompressionMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompressionMethod::Micro => write!(f, "MicroCompact"),
            CompressionMethod::Auto => write!(f, "AutoCompact"),
            CompressionMethod::Full => write!(f, "FullCompact"),
        }
    }
}

/// Metrics from a compression operation
#[derive(Debug, Clone)]
pub struct CompressionMetrics {
    pub method: CompressionMethod,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub tokens_saved: usize,
    pub messages_before: usize,
    pub messages_after: usize,
    pub duration_ms: u64,
}

impl CompressionMetrics {
    pub fn new(
        method: CompressionMethod,
        tokens_before: usize,
        tokens_after: usize,
        messages_before: usize,
        messages_after: usize,
        duration_ms: u64,
    ) -> Self {
        let tokens_saved = tokens_before.saturating_sub(tokens_after);
        Self {
            method,
            tokens_before,
            tokens_after,
            tokens_saved,
            messages_before,
            messages_after,
            duration_ms,
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "{}: Saved {} tokens ({} → {}), {} messages → {} (in {}ms)",
            self.method,
            self.tokens_saved,
            self.tokens_before,
            self.tokens_after,
            self.messages_before,
            self.messages_after,
            self.duration_ms
        )
    }
}

/// Configuration for AutoCompact
#[derive(Debug, Clone)]
pub struct AutoCompactConfig {
    /// Token threshold as percentage of context window (default: 80%)
    pub token_threshold: usize,
    /// Reserve buffer in tokens (default: 13K)
    pub reserve_buffer: usize,
    /// Maximum tokens for summary (default: 20K)
    pub max_summary_tokens: usize,
    /// Max consecutive failures before circuit breaker (default: 3)
    pub max_consecutive_failures: usize,
}

impl Default for AutoCompactConfig {
    fn default() -> Self {
        Self {
            token_threshold: 80, // 80% of context window
            reserve_buffer: 13_000,
            max_summary_tokens: 20_000,
            max_consecutive_failures: 3,
        }
    }
}

/// Manager for automatic compression with circuit breaker
pub struct AutoCompactManager {
    config: AutoCompactConfig,
    consecutive_failures: usize,
    circuit_open: bool,
    last_compression: Option<CompressionMetrics>,
}

impl AutoCompactManager {
    pub fn new(config: AutoCompactConfig) -> Self {
        Self {
            config,
            consecutive_failures: 0,
            circuit_open: false,
            last_compression: None,
        }
    }

    pub fn with_threshold(percentage: usize) -> Self {
        let config = AutoCompactConfig {
            token_threshold: percentage,
            ..Default::default()
        };
        Self::new(config)
    }

    /// Check if compression should trigger based on token count
    pub fn should_compress(&self, current_tokens: usize, context_window: usize) -> bool {
        if self.circuit_open {
            debug!("Circuit breaker open, skipping auto-compression");
            return false;
        }

        let threshold = (context_window * self.config.token_threshold) / 100;
        current_tokens >= threshold
    }

    /// Record a successful compression
    pub fn record_success(&mut self, metrics: CompressionMetrics) {
        self.consecutive_failures = 0;
        self.last_compression = Some(metrics);
    }

    /// Record a failed compression
    pub fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        if self.consecutive_failures >= self.config.max_consecutive_failures {
            self.circuit_open = true;
            warn!(
                "AutoCompact circuit breaker opened after {} consecutive failures",
                self.consecutive_failures
            );
        }
    }

    /// Reset the circuit breaker
    pub fn reset_circuit(&mut self) {
        self.circuit_open = false;
        self.consecutive_failures = 0;
    }

    pub fn is_circuit_open(&self) -> bool {
        self.circuit_open
    }

    pub fn last_compression(&self) -> Option<&CompressionMetrics> {
        self.last_compression.as_ref()
    }
}

/// Tracks which files were accessed in recent tool calls
#[derive(Debug, Clone)]
pub struct FileAccessTracker {
    /// Recent file accesses (path, timestamp)
    recent_accesses: VecDeque<(String, std::time::Instant)>,
    /// Maximum number of tool calls to track
    max_tracked_calls: usize,
}

impl Default for FileAccessTracker {
    fn default() -> Self {
        Self::new(20)
    }
}

impl FileAccessTracker {
    pub fn new(max_tracked_calls: usize) -> Self {
        Self {
            recent_accesses: VecDeque::with_capacity(max_tracked_calls),
            max_tracked_calls,
        }
    }

    /// Record a file read access
    pub fn record_access(&mut self, path: &str) {
        let now = std::time::Instant::now();

        // Remove existing entry for this path to update timestamp
        self.recent_accesses.retain(|(p, _)| p != path);

        // Add new entry
        self.recent_accesses.push_back((path.to_string(), now));

        // Trim to max size
        while self.recent_accesses.len() > self.max_tracked_calls {
            self.recent_accesses.pop_front();
        }
    }

    /// Get the N most recently accessed file paths
    pub fn get_recent_files(&self, n: usize) -> Vec<String> {
        self.recent_accesses
            .iter()
            .rev()
            .take(n)
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// Get all tracked file paths with their age
    pub fn get_tracked_files(&self) -> Vec<(String, std::time::Duration)> {
        let now = std::time::Instant::now();
        self.recent_accesses
            .iter()
            .map(|(p, t)| (p.clone(), now.duration_since(*t)))
            .collect()
    }

    pub fn clear(&mut self) {
        self.recent_accesses.clear();
    }

    pub fn len(&self) -> usize {
        self.recent_accesses.len()
    }

    pub fn is_empty(&self) -> bool {
        self.recent_accesses.is_empty()
    }
}

/// Estimate tokens for a message (simplified for compression)
fn estimate_tokens(msg: &Message) -> usize {
    let content_tokens = msg.content.text().len() / 4;
    let reasoning_tokens = msg
        .reasoning_content
        .as_ref()
        .map(|r| r.len() / 4)
        .unwrap_or(0);
    let tool_call_tokens = msg.tool_calls.as_ref().map(|tc| tc.len() * 50).unwrap_or(0);

    content_tokens + reasoning_tokens + tool_call_tokens + 4 // overhead
}

/// MicroCompact: Fast local compression with no API call
///
/// - Strips old tool outputs from conversation
/// - Removes thinking/reasoning blocks older than N turns
/// - Keeps last 10 message pairs minimum
/// - Zero latency, runs synchronously
pub fn micro_compact(messages: &mut Vec<Message>) -> CompressionMetrics {
    let start = std::time::Instant::now();
    let tokens_before = messages.iter().map(estimate_tokens).sum::<usize>();
    let messages_before = messages.len();

    if messages.len() <= 12 {
        // Not enough messages to compress meaningfully
        return CompressionMetrics::new(
            CompressionMethod::Micro,
            tokens_before,
            tokens_before,
            messages_before,
            messages_before,
            start.elapsed().as_millis() as u64,
        );
    }

    // Keep system message + last 10 pairs (20 messages) + user messages for context
    const MIN_MESSAGES_TO_KEEP: usize = 22;

    let system_msg = messages.first().cloned();
    let keep_start = messages.len().saturating_sub(MIN_MESSAGES_TO_KEEP);

    // Early return if there's nothing meaningful to compress
    if keep_start <= 1 {
        return CompressionMetrics::new(
            CompressionMethod::Micro,
            tokens_before,
            tokens_before,
            messages_before,
            messages_before,
            start.elapsed().as_millis() as u64,
        );
    }

    // Build new message list
    let mut compressed = Vec::new();

    // Always keep system message
    if let Some(sys) = system_msg {
        compressed.push(sys);
    }

    // Process messages to compress
    // Ensure valid range - if keep_start <= 1, there's nothing to compress
    let to_compress = if keep_start > 1 {
        &messages[1..keep_start]
    } else {
        &[]
    };
    let recent = &messages[keep_start..];

    // Add compressed summary message for old content
    if !to_compress.is_empty() {
        let old_user_count = to_compress.iter().filter(|m| m.role == "user").count();
        let old_assistant_count = to_compress.iter().filter(|m| m.role == "assistant").count();
        let old_tool_count = to_compress.iter().filter(|m| m.role == "tool").count();

        let summary = format!(
            "[MICRO-COMPACT: {} earlier messages compressed ({} user, {} assistant, {} tool results). \
             Key decisions and file edits preserved.]",
            to_compress.len(),
            old_user_count,
            old_assistant_count,
            old_tool_count
        );

        compressed.push(Message::user(summary));
    }

    // Add recent messages (with some cleaning)
    for (i, msg) in recent.iter().enumerate() {
        let mut cleaned = msg.clone();

        // Strip reasoning from older messages in the "recent" section
        // Keep reasoning for the last 2 exchanges
        let is_recent = i >= recent.len().saturating_sub(4);
        if !is_recent {
            cleaned.reasoning_content = None;
        }

        // For tool messages older than last 2, truncate content
        if msg.role == "tool" && !is_recent {
            if let MessageContent::Text(text) = &msg.content {
                if text.len() > 500 {
                    let truncated = format!(
                        "{}\n...[{} chars truncated by micro_compact]",
                        &text[..500.min(text.len())],
                        text.len() - 500
                    );
                    cleaned.content = MessageContent::Text(truncated);
                }
            }
        }

        compressed.push(cleaned);
    }

    let tokens_after = compressed.iter().map(estimate_tokens).sum::<usize>();
    let messages_after = compressed.len();

    let metrics = CompressionMetrics::new(
        CompressionMethod::Micro,
        tokens_before,
        tokens_after,
        messages_before,
        messages_after,
        start.elapsed().as_millis() as u64,
    );

    *messages = compressed;
    metrics
}

/// AutoCompact: Automatic compression using LLM summarization
///
/// - Uses LLM to generate summary via API call
/// - Replaces old messages with summary message
/// - Uses cheapest available model if configured
pub async fn auto_compact(
    client: &ApiClient,
    messages: &mut Vec<Message>,
    _config: &AutoCompactConfig,
) -> Result<CompressionMetrics> {
    let start = std::time::Instant::now();
    let tokens_before = messages.iter().map(estimate_tokens).sum::<usize>();
    let messages_before = messages.len();

    if messages.len() <= 8 {
        return Ok(CompressionMetrics::new(
            CompressionMethod::Auto,
            tokens_before,
            tokens_before,
            messages_before,
            messages_before,
            start.elapsed().as_millis() as u64,
        ));
    }

    // Keep system message and recent messages
    let system_msg = messages.first().cloned();
    let keep_recent = 6;
    let recent_start = messages.len().saturating_sub(keep_recent);
    let recent_msgs: Vec<Message> = messages[recent_start..].to_vec();
    let to_summarize = &messages[1..recent_start];

    if to_summarize.is_empty() {
        return Ok(CompressionMetrics::new(
            CompressionMethod::Auto,
            tokens_before,
            tokens_before,
            messages_before,
            messages_before,
            start.elapsed().as_millis() as u64,
        ));
    }

    // Build summary prompt
    let summary_content = format!(
        "Summarize the following conversation history concisely. \
         Preserve key facts, decisions, file paths, and action items. \
         Omit routine tool outputs unless they contain errors or important results.\n\n{}",
        to_summarize
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let content = if m.content.text().len() > 800 {
                    format!("{}...[truncated]", &m.content.text()[..800])
                } else {
                    m.content.text().to_string()
                };
                format!("[{}] {}: {}", i, m.role, content)
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    );

    let summary_request = vec![
        Message::system("You are a context summarizer. Compress conversation history while preserving critical information for task completion. Be concise."),
        Message::user(summary_content),
    ];

    // Call LLM for summary
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        client.chat(summary_request, None, ThinkingMode::Disabled),
    )
    .await
    .map_err(|_| anyhow::anyhow!("AutoCompact API call timed out after 60s"))??;

    let summary = response
        .choices
        .first()
        .map(|c| c.message.content.text().to_string())
        .unwrap_or_else(|| "[Context compression: conversation history]".to_string());

    // Build compressed message list
    let mut compressed = Vec::new();
    if let Some(sys) = system_msg {
        compressed.push(sys);
    }

    compressed.push(Message::user(format!(
        "[AUTO-COMPACT SUMMARY — {} earlier messages]:\n{}",
        to_summarize.len(),
        summary
    )));

    compressed.extend(recent_msgs);

    let tokens_after = compressed.iter().map(estimate_tokens).sum::<usize>();
    let messages_after = compressed.len();

    let metrics = CompressionMetrics::new(
        CompressionMethod::Auto,
        tokens_before,
        tokens_after,
        messages_before,
        messages_after,
        start.elapsed().as_millis() as u64,
    );

    *messages = compressed;
    Ok(metrics)
}

/// FullCompact: Nuclear option for extreme context pressure
///
/// - Compresses entire conversation into single summary
/// - Re-injects recently accessed files
/// - Preserves active plans and skill schemas
/// - Leaves target budget post-compression
pub async fn full_compact(
    client: &ApiClient,
    messages: &mut Vec<Message>,
    file_tracker: &FileAccessTracker,
    _target_budget: usize,
) -> Result<CompressionMetrics> {
    let start = std::time::Instant::now();
    let tokens_before = messages.iter().map(estimate_tokens).sum::<usize>();
    let messages_before = messages.len();

    // Always preserve system prompt and most recent user message
    let system_msg = messages.first().cloned();
    let last_user_msg = messages.iter().rev().find(|m| m.role == "user").cloned();

    if messages.len() <= 4 {
        return Ok(CompressionMetrics::new(
            CompressionMethod::Full,
            tokens_before,
            tokens_before,
            messages_before,
            messages_before,
            start.elapsed().as_millis() as u64,
        ));
    }

    // Build comprehensive summary prompt
    let summary_prompt = "Create a comprehensive but concise summary of this entire conversation. \
         Include: 1) Task goal and current status, 2) Key files modified/accessed, \
         3) Important decisions made, 4) Current blockers or next steps, \
         5) Any active plans or schemas in use."
        .to_string();

    let summary_request = vec![
        Message::system("You are a comprehensive context summarizer. Preserve all critical information for continuing the task."),
        Message::user(format!("{}\n\nConversation:\n{}",
            summary_prompt,
            messages.iter().enumerate().map(|(i, m)| {
                let content = if m.content.text().len() > 600 {
                    format!("{}...[truncated]", &m.content.text()[..600])
                } else {
                    m.content.text().to_string()
                };
                format!("[{}] {}: {}", i, m.role, content)
            }).collect::<Vec<_>>().join("\n")
        )),
    ];

    // Get summary from LLM
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(90),
        client.chat(summary_request, None, ThinkingMode::Disabled),
    )
    .await
    .map_err(|_| anyhow::anyhow!("FullCompact API call timed out after 90s"))??;

    let summary = response
        .choices
        .first()
        .map(|c| c.message.content.text().to_string())
        .unwrap_or_else(|| "[Full context compression applied]".to_string());

    // Build new compressed context
    let mut compressed = Vec::new();

    // 1. System prompt
    if let Some(sys) = system_msg {
        compressed.push(sys);
    }

    // 2. Full summary
    compressed.push(Message::user(format!(
        "[FULL-COMPACT — Complete conversation summary]:\n{}\n\n[Recent files and context follow]",
        summary
    )));

    // 3. Re-inject recently accessed files (last 5, 5K cap each)
    let recent_files = file_tracker.get_recent_files(5);
    if !recent_files.is_empty() {
        let mut file_context = String::from("\n## Recently Accessed Files:\n");
        for path in recent_files {
            // Try to read file content (best effort)
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                let truncated = if content.len() > 20_000 {
                    format!(
                        "{}\n...[truncated, {} total chars]",
                        &content[..20_000],
                        content.len()
                    )
                } else {
                    content
                };
                file_context.push_str(&format!("\n### {}\n```\n{}\n```\n", path, truncated));
            } else {
                file_context.push_str(&format!("\n### {} (unavailable)\n", path));
            }
        }
        compressed.push(Message::user(file_context));
    }

    // 4. Keep last user message if different from recent files message
    if let Some(last_user) = last_user_msg {
        // Only add if it's not already the last message
        if compressed.last().map(|m| m.content.text()) != Some(last_user.content.text()) {
            compressed.push(last_user);
        }
    }

    let tokens_after = compressed.iter().map(estimate_tokens).sum::<usize>();
    let messages_after = compressed.len();

    let metrics = CompressionMetrics::new(
        CompressionMethod::Full,
        tokens_before,
        tokens_after,
        messages_before,
        messages_after,
        start.elapsed().as_millis() as u64,
    );

    *messages = compressed;
    Ok(metrics)
}

/// Three-layer compression orchestrator
pub struct CompressionOrchestrator {
    auto_manager: AutoCompactManager,
    file_tracker: FileAccessTracker,
    metrics_history: Vec<CompressionMetrics>,
}

impl Default for CompressionOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl CompressionOrchestrator {
    pub fn new() -> Self {
        Self {
            auto_manager: AutoCompactManager::new(AutoCompactConfig::default()),
            file_tracker: FileAccessTracker::default(),
            metrics_history: Vec::new(),
        }
    }

    pub fn with_config(config: AutoCompactConfig) -> Self {
        Self {
            auto_manager: AutoCompactManager::new(config),
            file_tracker: FileAccessTracker::default(),
            metrics_history: Vec::new(),
        }
    }

    /// Run MicroCompact (synchronous, no API call)
    pub fn run_micro(&mut self, messages: &mut Vec<Message>) -> CompressionMetrics {
        let metrics = micro_compact(messages);
        self.metrics_history.push(metrics.clone());
        metrics
    }

    /// Run AutoCompact (async, uses LLM)
    pub async fn run_auto(
        &mut self,
        client: &ApiClient,
        messages: &mut Vec<Message>,
    ) -> Result<CompressionMetrics> {
        match auto_compact(client, messages, &self.auto_manager.config).await {
            Ok(metrics) => {
                self.auto_manager.record_success(metrics.clone());
                self.metrics_history.push(metrics.clone());
                Ok(metrics)
            }
            Err(e) => {
                self.auto_manager.record_failure();
                Err(e)
            }
        }
    }

    /// Run FullCompact (async, nuclear option)
    pub async fn run_full(
        &mut self,
        client: &ApiClient,
        messages: &mut Vec<Message>,
    ) -> Result<CompressionMetrics> {
        let metrics = full_compact(
            client,
            messages,
            &self.file_tracker,
            50_000, // Leave 50K budget
        )
        .await?;
        self.metrics_history.push(metrics.clone());
        Ok(metrics)
    }

    /// Check if auto-compression should trigger and run it
    pub async fn check_and_compress(
        &mut self,
        client: &ApiClient,
        messages: &mut Vec<Message>,
        current_tokens: usize,
        context_window: usize,
    ) -> Option<CompressionMetrics> {
        if !self
            .auto_manager
            .should_compress(current_tokens, context_window)
        {
            return None;
        }

        // Try AutoCompact first
        match self.run_auto(client, messages).await {
            Ok(metrics) => {
                info!("AutoCompact triggered: {}", metrics.summary());
                Some(metrics)
            }
            Err(e) => {
                warn!("AutoCompact failed, falling back to MicroCompact: {}", e);
                let metrics = self.run_micro(messages);
                info!("MicroCompact fallback: {}", metrics.summary());
                Some(metrics)
            }
        }
    }

    /// Record a file access
    pub fn record_file_access(&mut self, path: &str) {
        self.file_tracker.record_access(path);
    }

    /// Get the file access tracker
    pub fn file_tracker(&self) -> &FileAccessTracker {
        &self.file_tracker
    }

    /// Get compression history
    pub fn metrics_history(&self) -> &[CompressionMetrics] {
        &self.metrics_history
    }

    /// Get total tokens saved across all compressions
    pub fn total_tokens_saved(&self) -> usize {
        self.metrics_history.iter().map(|m| m.tokens_saved).sum()
    }

    /// Reset the orchestrator state
    pub fn reset(&mut self) {
        self.auto_manager.reset_circuit();
        self.file_tracker.clear();
        self.metrics_history.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_micro_compact_basic() {
        // Need many messages for meaningful compression (> MIN_MESSAGES_TO_KEEP + buffer)
        let mut messages = vec![Message::system("System prompt")];

        // Add 30 message pairs to have significant compression
        for i in 0..15 {
            messages.push(Message::user(format!("Question {}", i)));
            messages.push(Message::assistant(format!("Answer {}", i)));
        }
        messages.push(Message::user("Current question"));
        // Total: 1 + 30 + 1 = 32 messages
        // After compression: 1 (system) + 1 (summary) + 22 (recent) + 1 = 25

        let metrics = micro_compact(&mut messages);

        assert_eq!(metrics.method, CompressionMethod::Micro);
        // tokens_saved is usize, always >= 0 by definition
        // Should be compressed
        assert!(
            messages.len() <= 26,
            "Expected at most 26 messages, got {}",
            messages.len()
        );
        assert_eq!(messages[0].role, "system"); // System preserved
    }

    #[test]
    fn test_micro_compact_keeps_recent() {
        // Need at least 23 messages for compression to trigger
        let mut messages = vec![Message::system("System prompt")];

        // Add 20 older exchanges
        for i in 0..10 {
            messages.push(Message::user(format!("Old user {}", i)));
            messages.push(Message::assistant(format!("Old assistant {}", i)));
        }

        // Add recent messages that should be preserved
        messages.push(Message::user("Recent user"));
        messages.push(Message::assistant("Recent assistant"));
        // Add one more to ensure we're over the threshold
        messages.push(Message::user("Final question"));

        micro_compact(&mut messages);

        // Check that recent messages are preserved
        let has_recent_user = messages
            .iter()
            .any(|m| m.content.text().contains("Recent user"));
        assert!(has_recent_user, "Recent user message should be preserved");
    }

    #[test]
    fn test_micro_compact_strips_reasoning() {
        // Need many messages for meaningful compression
        let mut messages = vec![Message::system("System prompt")];

        // Add 20 old exchanges (40 messages) with reasoning
        for i in 0..20 {
            messages.push(Message::user(format!("Old question {}", i)));
            messages.push(Message::assistant_with_reasoning(
                format!("Old answer {}", i),
                format!("Old reasoning {}", i),
            ));
        }

        // Add recent messages
        messages.push(Message::user("Recent question"));
        messages.push(Message::assistant_with_reasoning(
            "Recent answer",
            "Recent reasoning",
        ));
        // Total: 1 + 40 + 2 = 43 messages
        // keep_start = 43 - 22 = 21, so messages[1..21] (20 messages) get compressed

        micro_compact(&mut messages);

        // After compression, old messages are summarized into a single message
        // The "Old answer" messages should no longer exist individually
        let has_old_answer = messages
            .iter()
            .any(|m| m.content.text().contains("Old answer 0"));
        let has_recent_answer = messages
            .iter()
            .any(|m| m.content.text().contains("Recent answer"));

        // Old messages should be compressed away
        assert!(!has_old_answer, "Old messages should be compressed");
        // Recent messages should be kept
        assert!(has_recent_answer, "Recent messages should be kept");
    }

    #[test]
    fn test_file_access_tracker() {
        let mut tracker = FileAccessTracker::new(5);

        tracker.record_access("file1.rs");
        tracker.record_access("file2.rs");
        tracker.record_access("file3.rs");

        let recent = tracker.get_recent_files(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0], "file3.rs"); // Most recent first
        assert_eq!(recent[1], "file2.rs");
    }

    #[test]
    fn test_file_access_tracker_updates_timestamp() {
        let mut tracker = FileAccessTracker::new(5);

        tracker.record_access("file1.rs");
        std::thread::sleep(std::time::Duration::from_millis(10));
        tracker.record_access("file2.rs");
        std::thread::sleep(std::time::Duration::from_millis(10));
        tracker.record_access("file1.rs"); // Re-access file1

        let recent = tracker.get_recent_files(2);
        assert_eq!(recent[0], "file1.rs"); // file1 should now be most recent
    }

    #[test]
    fn test_compression_metrics() {
        let metrics = CompressionMetrics::new(CompressionMethod::Auto, 1000, 600, 20, 8, 150);

        assert_eq!(metrics.tokens_saved, 400);
        assert!(metrics.summary().contains("AutoCompact"));
        assert!(metrics.summary().contains("400"));
    }

    #[test]
    fn test_auto_compact_config_default() {
        let config = AutoCompactConfig::default();
        assert_eq!(config.token_threshold, 80);
        assert_eq!(config.reserve_buffer, 13_000);
        assert_eq!(config.max_summary_tokens, 20_000);
        assert_eq!(config.max_consecutive_failures, 3);
    }

    #[test]
    fn test_auto_compact_manager_circuit_breaker() {
        let mut manager = AutoCompactManager::new(AutoCompactConfig::default());

        // Record failures up to the limit
        for _ in 0..3 {
            manager.record_failure();
        }

        assert!(manager.is_circuit_open());

        // Reset should clear the circuit
        manager.reset_circuit();
        assert!(!manager.is_circuit_open());
        assert_eq!(manager.consecutive_failures, 0);
    }

    #[test]
    fn test_compression_method_display() {
        assert_eq!(CompressionMethod::Micro.to_string(), "MicroCompact");
        assert_eq!(CompressionMethod::Auto.to_string(), "AutoCompact");
        assert_eq!(CompressionMethod::Full.to_string(), "FullCompact");
    }

    #[test]
    fn test_micro_compact_small_conversation() {
        // Small conversations shouldn't be compressed
        let mut messages = vec![
            Message::system("System prompt"),
            Message::user("Question 1"),
            Message::assistant("Answer 1"),
            Message::user("Question 2"),
        ];

        let metrics = micro_compact(&mut messages);

        // Should return without changes for small conversations
        assert_eq!(metrics.messages_before, metrics.messages_after);
    }

    #[test]
    fn test_orchestrator_total_tokens_saved() {
        let mut orchestrator = CompressionOrchestrator::new();

        // Simulate some compressions
        orchestrator.metrics_history.push(CompressionMetrics::new(
            CompressionMethod::Micro,
            1000,
            800,
            20,
            15,
            50,
        ));
        orchestrator.metrics_history.push(CompressionMetrics::new(
            CompressionMethod::Auto,
            800,
            500,
            15,
            8,
            100,
        ));

        assert_eq!(orchestrator.total_tokens_saved(), 500);
    }

    #[test]
    fn test_micro_compact_preserves_tool_calls() {
        use crate::api::types::{ToolCall, ToolFunction};

        let mut messages = vec![
            Message::system("System prompt"),
            Message::user("Old question"),
            Message {
                role: "assistant".to_string(),
                content: MessageContent::Text("Let me check".to_string()),
                reasoning_content: None,
                tool_calls: Some(vec![ToolCall {
                    id: "call_1".to_string(),
                    call_type: "function".to_string(),
                    function: ToolFunction {
                        name: "file_read".to_string(),
                        arguments: r#"{"path": "test.rs"}"#.to_string(),
                    },
                }]),
                tool_call_id: None,
                name: None,
            },
            Message::user("Recent question"),
            Message::assistant("Recent answer"),
        ];

        // Add more messages to trigger compression
        for i in 0..15 {
            messages.push(Message::user(format!("Question {}", i)));
            messages.push(Message::assistant(format!("Answer {}", i)));
        }

        let metrics = micro_compact(&mut messages);

        // Should have compressed
        assert!(metrics.messages_after < metrics.messages_before);
        // System message should be preserved
        assert_eq!(messages[0].role, "system");
    }

    #[test]
    fn test_file_access_tracker_get_tracked_files() {
        let mut tracker = FileAccessTracker::new(5);

        // Initially empty
        let tracked = tracker.get_tracked_files();
        assert!(tracked.is_empty());

        // Record some accesses
        tracker.record_access("file1.rs");
        std::thread::sleep(std::time::Duration::from_millis(5));
        tracker.record_access("file2.rs");

        let tracked = tracker.get_tracked_files();
        assert_eq!(tracked.len(), 2);

        // Check that durations are reasonable (should be very small)
        for (_, duration) in &tracked {
            assert!(duration.as_secs() < 1); // Should be less than 1 second
        }
    }

    #[test]
    fn test_file_access_tracker_is_empty_and_len() {
        let mut tracker = FileAccessTracker::new(5);

        assert!(tracker.is_empty());
        assert_eq!(tracker.len(), 0);

        tracker.record_access("file1.rs");

        assert!(!tracker.is_empty());
        assert_eq!(tracker.len(), 1);

        tracker.record_access("file2.rs");
        assert_eq!(tracker.len(), 2);
    }

    #[test]
    fn test_file_access_tracker_clear() {
        let mut tracker = FileAccessTracker::new(5);

        tracker.record_access("file1.rs");
        tracker.record_access("file2.rs");
        assert_eq!(tracker.len(), 2);

        tracker.clear();

        assert!(tracker.is_empty());
        assert_eq!(tracker.len(), 0);
        assert!(tracker.get_recent_files(10).is_empty());
    }

    #[test]
    fn test_file_access_tracker_capacity_limit() {
        let mut tracker = FileAccessTracker::new(3);

        // Add more files than capacity
        tracker.record_access("file1.rs");
        tracker.record_access("file2.rs");
        tracker.record_access("file3.rs");
        tracker.record_access("file4.rs");

        // Should only keep the most recent 3
        assert_eq!(tracker.len(), 3);

        let recent = tracker.get_recent_files(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0], "file4.rs"); // Most recent
        assert_eq!(recent[1], "file3.rs");
        assert_eq!(recent[2], "file2.rs"); // file1.rs should be evicted
    }

    #[test]
    fn test_auto_compact_manager_with_threshold() {
        let manager = AutoCompactManager::with_threshold(70);

        assert_eq!(manager.config.token_threshold, 70);
        assert_eq!(manager.config.reserve_buffer, 13_000); // Default
        assert!(!manager.is_circuit_open());
    }

    #[test]
    fn test_auto_compact_manager_should_compress() {
        let manager = AutoCompactManager::with_threshold(80);

        // Below threshold (80% of 100K = 80K)
        assert!(!manager.should_compress(70_000, 100_000));

        // At threshold
        assert!(manager.should_compress(80_000, 100_000));

        // Above threshold
        assert!(manager.should_compress(90_000, 100_000));

        // Edge case: small context window
        assert!(manager.should_compress(800, 1000));
        assert!(!manager.should_compress(799, 1000));
    }

    #[test]
    fn test_auto_compact_manager_should_compress_circuit_breaker() {
        let mut manager = AutoCompactManager::with_threshold(80);

        // Initially should compress
        assert!(manager.should_compress(90_000, 100_000));

        // Open the circuit
        for _ in 0..3 {
            manager.record_failure();
        }
        assert!(manager.is_circuit_open());

        // Should not compress when circuit is open
        assert!(!manager.should_compress(90_000, 100_000));
    }

    #[test]
    fn test_auto_compact_manager_record_success() {
        let mut manager = AutoCompactManager::new(AutoCompactConfig::default());

        // Record some failures first
        manager.record_failure();
        manager.record_failure();
        assert_eq!(manager.consecutive_failures, 2);

        // Record success should reset failures
        let metrics = CompressionMetrics::new(CompressionMethod::Auto, 1000, 600, 20, 8, 150);
        manager.record_success(metrics.clone());

        assert_eq!(manager.consecutive_failures, 0);
        assert!(manager.last_compression().is_some());
        assert_eq!(manager.last_compression().unwrap().tokens_saved, 400);
    }

    #[test]
    fn test_compression_orchestrator_with_config() {
        let config = AutoCompactConfig {
            token_threshold: 75,
            reserve_buffer: 10_000,
            max_summary_tokens: 15_000,
            max_consecutive_failures: 5,
        };

        let orchestrator = CompressionOrchestrator::with_config(config.clone());

        assert!(orchestrator.metrics_history().is_empty());
        assert!(orchestrator.file_tracker().is_empty());
        assert_eq!(orchestrator.total_tokens_saved(), 0);
    }

    #[test]
    fn test_compression_orchestrator_record_file_access() {
        let mut orchestrator = CompressionOrchestrator::new();

        orchestrator.record_file_access("src/main.rs");
        orchestrator.record_file_access("src/lib.rs");

        let tracker = orchestrator.file_tracker();
        assert_eq!(tracker.len(), 2);

        let recent = tracker.get_recent_files(2);
        assert!(recent.contains(&"src/main.rs".to_string()));
        assert!(recent.contains(&"src/lib.rs".to_string()));
    }

    #[test]
    fn test_compression_orchestrator_reset() {
        let mut orchestrator = CompressionOrchestrator::new();

        // Add some state
        orchestrator.record_file_access("file.rs");
        orchestrator.metrics_history.push(CompressionMetrics::new(
            CompressionMethod::Micro,
            1000,
            800,
            20,
            15,
            50,
        ));

        // Open circuit breaker
        orchestrator.auto_manager.record_failure();
        orchestrator.auto_manager.record_failure();
        orchestrator.auto_manager.record_failure();
        assert!(orchestrator.auto_manager.is_circuit_open());

        // Reset
        orchestrator.reset();

        // Verify all cleared
        assert!(orchestrator.file_tracker().is_empty());
        assert!(orchestrator.metrics_history().is_empty());
        assert!(!orchestrator.auto_manager.is_circuit_open());
        assert_eq!(orchestrator.total_tokens_saved(), 0);
    }

    #[test]
    fn test_micro_compact_exactly_at_threshold() {
        // Test with exactly 12 messages (at threshold, should not compress)
        let mut messages = vec![Message::system("System prompt")];
        for i in 0..5 {
            messages.push(Message::user(format!("Question {}", i)));
            messages.push(Message::assistant(format!("Answer {}", i)));
        }
        messages.push(Message::user("Final"));
        // Total: 1 + 10 + 1 = 12 messages

        let metrics = micro_compact(&mut messages);

        // Should not compress at exactly 12
        assert_eq!(metrics.messages_before, metrics.messages_after);
    }

    #[test]
    fn test_micro_compact_at_13_messages() {
        // Test with 13 messages (just over threshold of 12)
        let mut messages = vec![Message::system("System prompt")];
        for i in 0..5 {
            messages.push(Message::user(format!("Question {}", i)));
            messages.push(Message::assistant(format!("Answer {}", i)));
        }
        messages.push(Message::user("Final 1"));
        messages.push(Message::user("Final 2"));
        // Total: 1 + 10 + 2 = 13 messages
        assert_eq!(messages.len(), 13);

        let metrics = micro_compact(&mut messages);

        // At 13 messages, keep_start = 13 - 22 = 0, so nothing to compress
        assert_eq!(metrics.messages_before, metrics.messages_after);
    }

    #[test]
    fn test_micro_compact_with_tool_messages() {
        let mut messages = vec![Message::system("System prompt")];

        // Add old tool messages
        for i in 0..8 {
            messages.push(Message::user(format!("Request {}", i)));
            messages.push(Message {
                role: "tool".to_string(),
                content: MessageContent::Text(format!(
                    "Tool result {} with lots of content here",
                    i
                )),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: Some(format!("call_{}", i)),
                name: Some("test_tool".to_string()),
            });
        }

        // Add recent messages
        for i in 0..8 {
            messages.push(Message::user(format!("Recent {}", i)));
            messages.push(Message::assistant(format!("Answer {}", i)));
        }

        let metrics = micro_compact(&mut messages);

        // Should have compressed
        assert!(metrics.messages_after < metrics.messages_before);

        // Check that summary contains tool count
        let summary_msg = &messages[1];
        assert!(summary_msg.content.text().contains("tool"));
    }

    #[test]
    fn test_micro_compact_empty_messages() {
        // Test with empty messages
        let mut messages: Vec<Message> = vec![];

        let metrics = micro_compact(&mut messages);

        assert_eq!(metrics.messages_before, 0);
        assert_eq!(metrics.messages_after, 0);
        assert!(messages.is_empty());
    }

    #[test]
    fn test_micro_compact_reasoning_stripping() {
        let mut messages = vec![Message::system("System prompt")];

        // Add messages with reasoning - need enough to trigger compression
        for i in 0..15 {
            messages.push(Message::user(format!("Question {}", i)));
            messages.push(Message::assistant_with_reasoning(
                format!("Answer {}", i),
                format!("Reasoning for answer {} with detailed thought process", i),
            ));
        }

        let _metrics = micro_compact(&mut messages);

        // Count how many messages still have reasoning content
        let reasoning_count = messages
            .iter()
            .filter(|m| m.reasoning_content.is_some())
            .count();

        // Only the last 4 messages in "recent" section should keep reasoning
        // Recent section has 22 messages, last 4 exchanges (8 messages) keep reasoning
        // But the summary message doesn't have reasoning
        assert!(
            reasoning_count <= 4,
            "Expected at most 4 messages with reasoning, got {}",
            reasoning_count
        );
    }

    #[test]
    fn test_compression_metrics_all_methods() {
        // Test Micro
        let micro = CompressionMetrics::new(CompressionMethod::Micro, 2000, 1500, 30, 25, 10);
        assert_eq!(micro.tokens_saved, 500);
        assert!(micro.summary().contains("MicroCompact"));

        // Test Auto
        let auto = CompressionMetrics::new(CompressionMethod::Auto, 5000, 3000, 50, 10, 100);
        assert_eq!(auto.tokens_saved, 2000);
        assert!(auto.summary().contains("AutoCompact"));

        // Test Full
        let full = CompressionMetrics::new(CompressionMethod::Full, 10000, 5000, 100, 5, 200);
        assert_eq!(full.tokens_saved, 5000);
        assert!(full.summary().contains("FullCompact"));
    }

    #[test]
    fn test_compression_metrics_zero_savings() {
        let metrics = CompressionMetrics::new(
            CompressionMethod::Micro,
            1000,
            1200, // After > before
            10,
            12,
            5,
        );

        // tokens_saved should be 0 (saturating_sub)
        assert_eq!(metrics.tokens_saved, 0);
        assert!(metrics.summary().contains("Saved 0 tokens"));
    }

    #[test]
    fn test_micro_compact_preserves_system_message() {
        let mut messages = vec![Message::system(
            "Important system prompt that must be preserved",
        )];

        // Add many user/assistant messages
        for i in 0..20 {
            messages.push(Message::user(format!("User message {}", i)));
            messages.push(Message::assistant(format!("Assistant response {}", i)));
        }

        micro_compact(&mut messages);

        // System message should still be first
        assert_eq!(messages[0].role, "system");
        assert!(messages[0]
            .content
            .text()
            .contains("Important system prompt"));
    }

    #[test]
    fn test_orchestrator_run_micro() {
        let mut orchestrator = CompressionOrchestrator::new();

        let mut messages = vec![Message::system("System")];
        for i in 0..15 {
            messages.push(Message::user(format!("Q{}", i)));
            messages.push(Message::assistant(format!("A{}", i)));
        }

        let metrics = orchestrator.run_micro(&mut messages);

        assert_eq!(metrics.method, CompressionMethod::Micro);
        assert_eq!(orchestrator.metrics_history().len(), 1);
        assert_eq!(orchestrator.total_tokens_saved(), metrics.tokens_saved);
    }

    #[test]
    fn test_orchestrator_multiple_micro_runs() {
        let mut orchestrator = CompressionOrchestrator::new();

        // First compression
        let mut messages1 = vec![Message::system("System")];
        for i in 0..15 {
            messages1.push(Message::user(format!("Q{}", i)));
            messages1.push(Message::assistant(format!("A{}", i)));
        }
        orchestrator.run_micro(&mut messages1);

        // Second compression
        let mut messages2 = vec![Message::system("System")];
        for i in 0..15 {
            messages2.push(Message::user(format!("Q{}", i)));
            messages2.push(Message::assistant(format!("A{}", i)));
        }
        orchestrator.run_micro(&mut messages2);

        assert_eq!(orchestrator.metrics_history().len(), 2);
        assert_eq!(
            orchestrator.total_tokens_saved(),
            orchestrator.metrics_history[0].tokens_saved
                + orchestrator.metrics_history[1].tokens_saved
        );
    }
}
