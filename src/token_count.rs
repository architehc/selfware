//! Shared token counting utilities.
//!
//! Uses `tokenizers` for accurate counts against Qwen models, falls back to `tiktoken-rs`
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

// Try to load Qwen tokenizer, fallback to OpenAI cl100k.
// TokenizerState is Send + Sync (both Tokenizer and CoreBPE are), and
// count() only requires &self, so no Mutex is needed — Lazy alone provides
// safe one-time initialization and lock-free concurrent reads.
static TOKENIZER: Lazy<TokenizerState> = Lazy::new(TokenizerState::new);

/// Thread-safe cache mapping content hash (u64) -> token count.
static TOKEN_CACHE: Lazy<RwLock<HashMap<u64, usize>>> =
    Lazy::new(|| RwLock::new(HashMap::with_capacity(256)));

enum TokenizerState {
    Qwen(Box<Tokenizer>),
    Tiktoken(CoreBPE),
    Heuristic,
}

impl TokenizerState {
    fn new() -> Self {
        // Try to load Qwen tokenizer from HF hub
        // We use a known Qwen2.5-Coder or similar repo since they share the same vocabulary
        match Tokenizer::from_pretrained("Qwen/Qwen2.5-Coder-32B", None) {
            Ok(tokenizer) => {
                debug!("Successfully loaded Qwen tokenizer from HF Hub");
                return TokenizerState::Qwen(Box::new(tokenizer));
            }
            Err(e) => {
                warn!(
                    "Failed to load Qwen tokenizer: {}. Falling back to tiktoken cl100k",
                    e
                );
            }
        }

        match cl100k_base() {
            Ok(bpe) => TokenizerState::Tiktoken(bpe),
            Err(_) => TokenizerState::Heuristic,
        }
    }

    fn count(&self, content: &str) -> usize {
        match self {
            TokenizerState::Qwen(t) => t
                .encode(content, false)
                .map(|e| e.get_tokens().len())
                .unwrap_or_else(|_| heuristic_estimate(content)),
            TokenizerState::Tiktoken(bpe) => bpe.encode_with_special_tokens(content).len(),
            TokenizerState::Heuristic => heuristic_estimate(content),
        }
    }
}

/// Estimate token count for content and add a fixed per-message overhead.
#[inline]
pub fn estimate_tokens_with_overhead(content: &str, message_overhead: usize) -> usize {
    estimate_content_tokens(content) + message_overhead
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
mod tests {
    use super::*;

    #[test]
    fn test_estimate_content_tokens_non_zero() {
        let tokens = estimate_content_tokens("hello world");
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_tokens_with_overhead() {
        let tokens = estimate_tokens_with_overhead("hello", 10);
        assert!(tokens >= 11);
    }

    #[test]
    fn test_estimate_content_tokens_code() {
        let tokens = estimate_content_tokens("fn main() { println!(\"hi\"); }");
        assert!(tokens > 0);
    }

    #[test]
    fn test_cache_returns_consistent_results() {
        let content = "The quick brown fox jumps over the lazy dog";
        let first = estimate_content_tokens(content);
        // Second call should hit the cache and return the same value.
        let second = estimate_content_tokens(content);
        assert_eq!(first, second);
    }

    #[test]
    fn test_hash_content_deterministic() {
        let a = hash_content("hello");
        let b = hash_content("hello");
        assert_eq!(a, b);

        let c = hash_content("world");
        assert_ne!(a, c);
    }
}
