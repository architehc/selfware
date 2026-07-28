//! Shared token counting utilities.
//!
//! Tries to load a Hugging Face tokenizer matching the configured model
//! family, falls back to `tiktoken-rs` cl100k_base as a generic approximation,
//! and finally to a conservative heuristic if tokenizer initialization fails.
//!
//! A per-content hash cache avoids redundant tokenization for repeated strings.
//! The cache is capped at a fixed size and cleared entirely when full
//! (simple eviction that avoids the overhead of an LRU bookkeeping structure).

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::RwLock;
use tiktoken_rs::{cl100k_base, CoreBPE};
use tokenizers::Tokenizer;
use tracing::{debug, warn};

/// Maximum number of cached token counts before the cache is cleared.
const MAX_CACHE_ENTRIES: usize = 1_000;

/// Model name registered by the CLI entry point once it has loaded the
/// effective [`Config`](crate::config::Config) (see `set_configured_model`).
/// The tokenizer must never reload the config file itself: a mid-session
/// `Config::load(None)` ignored `--config`/`-c` and printed a spurious
/// `config: <path>` line into the session output (P2-11).
static CONFIGURED_MODEL: RwLock<Option<String>> = RwLock::new(None);

/// Record the model name from the already-loaded config so the tokenizer can
/// pick a matching HF tokenizer. Call once, early at startup — the tokenizer
/// is built lazily on first use, so registration after the first token
/// count has no effect.
pub fn set_configured_model(model: &str) {
    if let Ok(mut slot) = CONFIGURED_MODEL.write() {
        *slot = Some(model.to_string());
    }
}

/// Resolve the model whose tokenizer we should try to match.
/// Precedence: `SELFWARE_MODEL` env var, then the registered config model.
fn configured_model_name() -> Option<String> {
    std::env::var("SELFWARE_MODEL")
        .ok()
        .or_else(|| CONFIGURED_MODEL.read().ok().and_then(|slot| slot.clone()))
}

// Try to load a tokenizer matching the configured model, fall back to tiktoken
// cl100k_base, and finally to a heuristic.  TokenizerState is Send + Sync
// (both Tokenizer and CoreBPE are), and count() only requires &self, so no
// Mutex is needed — Lazy alone provides safe one-time initialization and
// lock-free concurrent reads.
static TOKENIZER: Lazy<TokenizerState> =
    Lazy::new(|| TokenizerState::for_model(configured_model_name().as_deref()));

/// Thread-safe cache mapping content hash (u64) -> token count.
static TOKEN_CACHE: Lazy<RwLock<HashMap<u64, usize>>> =
    Lazy::new(|| RwLock::new(HashMap::with_capacity(256)));

enum TokenizerState {
    /// HuggingFace tokenizer matched to the configured model family.
    Hf(Box<Tokenizer>),
    /// tiktoken cl100k_base — a reasonable generic approximation.
    Tiktoken(CoreBPE),
    /// Last-resort heuristic when no tokenizer can be loaded.
    Heuristic,
}

impl TokenizerState {
    /// Build a tokenizer state appropriate for the given model name.
    ///
    /// If `model` is `None` or we cannot determine a matching HF tokenizer
    /// repo, we fall back to the tiktoken cl100k_base encoding, which is a
    /// reasonable generic token-count approximation for most modern LLMs.
    /// If that also fails, we use a character-based heuristic.
    ///
    /// This never panics — every fallthrough path produces a usable state.
    fn for_model(model: Option<&str>) -> Self {
        // Try to load a model-specific HF tokenizer when a model name is
        // provided and looks like a HF repo id (contains '/') or a known
        // family prefix.
        if let Some(model) = model {
            if let Some(repo) = hf_tokenizer_repo(model) {
                match Tokenizer::from_pretrained(repo, None) {
                    Ok(tokenizer) => {
                        debug!("Loaded HF tokenizer '{}' for model '{}'", repo, model);
                        return TokenizerState::Hf(Box::new(tokenizer));
                    }
                    Err(e) => {
                        debug!(
                            "Could not load HF tokenizer '{}' for model '{}': {}, \
                             falling back to tiktoken cl100k",
                            repo, model, e
                        );
                    }
                }
            } else {
                debug!(
                    "No HF tokenizer repo mapped for model '{}', using cl100k fallback",
                    model
                );
            }
        }

        // Generic fallback: tiktoken cl100k_base
        match cl100k_base() {
            Ok(bpe) => {
                debug!("Using tiktoken cl100k_base tokenizer as fallback");
                TokenizerState::Tiktoken(bpe)
            }
            Err(e) => {
                warn!(
                    "Failed to initialize tiktoken cl100k_base: {}. \
                     Using heuristic token estimate.",
                    e
                );
                TokenizerState::Heuristic
            }
        }
    }

    fn count(&self, content: &str) -> usize {
        match self {
            TokenizerState::Hf(t) => t
                .encode(content, false)
                .map(|e| e.get_tokens().len())
                .unwrap_or_else(|_| heuristic_estimate(content)),
            TokenizerState::Tiktoken(bpe) => bpe.encode_with_special_tokens(content).len(),
            TokenizerState::Heuristic => heuristic_estimate(content),
        }
    }
}

/// Map a model name to a Hugging Face tokenizer repo id when the family is
/// known. Returns `None` for unrecognized models (caller falls back to
/// cl100k).
fn hf_tokenizer_repo(model: &str) -> Option<&'static str> {
    let lower = model.to_ascii_lowercase();
    if lower.contains("qwen") {
        // All Qwen2.5 variants share the same tokenizer vocabulary.
        Some("Qwen/Qwen2.5-Coder-32B")
    } else if lower.contains("gpt-4") || lower.contains("gpt4") {
        Some("Xenova/gpt-4")
    } else if lower.contains("gpt-3.5") || lower.contains("gpt-3") {
        Some("Xenova/gpt-3.5-turbo")
    } else if lower.contains("llama") {
        Some("hf-internal-testing/llama-tokenizer")
    } else if lower.contains("glm") {
        // GLM-4/5 family — use the public GLM-4 tokenizer which is
        // compatible. If this repo is unavailable the caller falls back
        // to cl100k.
        Some("THUDM/glm-4-9b-chat")
    } else if lower.contains("mistral") || lower.contains("mixtral") {
        Some("mistralai/Mistral-7B-v0.1")
    } else if lower.contains("deepseek") {
        Some("deepseek-ai/deepseek-coder-7b-instruct-v1.5")
    } else {
        None
    }
}

/// Estimate token count for content and add a fixed per-message overhead.
#[inline]
pub fn estimate_tokens_with_overhead(content: &str, message_overhead: usize) -> usize {
    estimate_content_tokens(content) + message_overhead
}

/// Default token estimate per image when dimensions are unknown.
/// Based on a 1024×1024 high-detail image: 4 tiles × 170 + 85 = 765.
pub const DEFAULT_IMAGE_TOKEN_ESTIMATE: usize = 765;

/// Estimate tokens for a string (never returns 0).
pub fn estimate_tokens(text: &str) -> usize {
    estimate_content_tokens(text).max(1)
}

/// Estimate tokens for a list of messages
pub fn estimate_messages_tokens(messages: &[crate::api::types::Message]) -> usize {
    let mut total = 0;

    for msg in messages {
        // Role overhead
        total += 4;
        // Content (use text_all to capture all text blocks)
        total += estimate_tokens(&msg.content.text_all());
        // Image tokens
        total += msg.content.image_count() * DEFAULT_IMAGE_TOKEN_ESTIMATE;
        // Tool calls if present
        if let Some(ref tool_calls) = msg.tool_calls {
            for call in tool_calls {
                total += 10; // Overhead per tool call
                total += estimate_tokens(&call.function.name);
                total += estimate_tokens(&call.function.arguments);
            }
        }
    }

    total
}

/// Estimate the token count of tool definitions sent via native function calling.
/// vLLM/OpenAI count these as input tokens, so they must be included in budget calculations.
pub fn estimate_tool_definitions_tokens(tools: &[crate::api::types::ToolDefinition]) -> usize {
    let mut total = 0;
    for tool in tools {
        // Each tool definition has overhead + name + description + parameter schema
        total += 10; // structural overhead (type, function wrapper)
        total += estimate_tokens(&tool.function.name);
        total += estimate_tokens(&tool.function.description);
        // Parameter schema JSON — serialize and estimate
        let schema_str = serde_json::to_string(&tool.function.parameters).unwrap_or_default();
        total += estimate_tokens(&schema_str);
    }
    total
}

/// Estimate tokens for raw content.
///
/// Results are cached by content hash to avoid redundant tokenization.
#[inline]
pub fn estimate_content_tokens(content: &str) -> usize {
    let key = hash_content(content);

    // Fast path: check the read-locked cache first.
    if let Ok(cache) = TOKEN_CACHE.read() {
        if let Some(&count) = cache.get(&key) {
            return count;
        }
    }

    // Cache miss — compute the token count.
    let count = TOKENIZER.count(content);

    // Store in cache (acquire write lock).
    if let Ok(mut cache) = TOKEN_CACHE.write() {
        // Simple eviction: clear when full rather than tracking LRU order.
        if cache.len() >= MAX_CACHE_ENTRIES {
            cache.clear();
        }
        cache.insert(key, count);
    }

    count
}

/// Compute a fast 64-bit hash of the content string for cache keying.
fn hash_content(content: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

fn heuristic_estimate(content: &str) -> usize {
    // Heuristic fallback that remains biased toward overestimation for safety.
    let factor = if content.contains('{') || content.contains(';') {
        3
    } else {
        4
    };
    (content.len() / factor).max(1)
}

#[cfg(test)]
#[path = "../tests/unit/token_count/token_count_test.rs"]
mod tests;
