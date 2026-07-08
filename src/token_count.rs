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

// Try to load a tokenizer matching the configured model, fall back to tiktoken
// cl100k_base, and finally to a heuristic.  TokenizerState is Send + Sync
// (both Tokenizer and CoreBPE are), and count() only requires &self, so no
// Mutex is needed — Lazy alone provides safe one-time initialization and
// lock-free concurrent reads.
static TOKENIZER: Lazy<TokenizerState> = Lazy::new(|| {
    // Determine the configured model name (if any) so we can pick a matching
    // tokenizer instead of hardcoding one.  We deliberately swallow config
    // load errors — token counting is best-effort and must never panic.
    let configured_model = std::env::var("SELFWARE_MODEL").ok().or_else(|| {
        crate::config::Config::load(None)
            .ok()
            .map(|c| c.model)
    });

    TokenizerState::for_model(configured_model.as_deref())
});

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
                        debug!(
                            "Loaded HF tokenizer '{}' for model '{}'",
                            repo, model
                        );
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

    #[test]
    fn test_hf_tokenizer_repo_known_families() {
        // Qwen models should map to a Qwen tokenizer repo
        assert!(hf_tokenizer_repo("Qwen/Qwen2.5-Coder-32B").is_some());
        assert!(hf_tokenizer_repo("qwen2.5-7b").is_some());

        // GLM models should map to a GLM tokenizer repo
        assert!(hf_tokenizer_repo("z-ai/glm-5.2").is_some());
        assert!(hf_tokenizer_repo("THUDM/glm-4-9b").is_some());

        // Other known families
        assert!(hf_tokenizer_repo("gpt-4o").is_some());
        assert!(hf_tokenizer_repo("llama-3-8b").is_some());
        assert!(hf_tokenizer_repo("mistral-7b").is_some());
    }

    #[test]
    fn test_hf_tokenizer_repo_unknown_returns_none() {
        // Unknown model families should return None so the caller
        // falls back to cl100k instead of hardcoding a tokenizer.
        assert!(hf_tokenizer_repo("some-random-model").is_none());
        assert!(hf_tokenizer_repo("").is_none());
    }

    #[test]
    fn test_for_model_falls_back_gracefully() {
        // Using a model name with no mapped HF repo should still produce
        // a working TokenizerState (cl100k or heuristic) — never panic.
        let state = TokenizerState::for_model(Some("unknown-model-xyz"));
        let count = state.count("fn main() { println!(\"hello\"); }");
        assert!(count > 0, "fallback tokenizer must produce a positive count");
    }

    #[test]
    fn test_for_model_none_falls_back_gracefully() {
        // No model at all should still work — cl100k or heuristic.
        let state = TokenizerState::for_model(None);
        let count = state.count("hello world");
        assert!(count > 0);
    }
}
