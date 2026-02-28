//! Shared token counting utilities.
//!
//! Uses `tokenizers` for accurate counts against Qwen models, falls back to `tiktoken-rs`
//! and finally to a conservative heuristic if tokenizer initialization fails.

use once_cell::sync::Lazy;
use tiktoken_rs::{cl100k_base, CoreBPE};
use tokenizers::Tokenizer;
use tracing::{debug, warn};

// Try to load Qwen tokenizer, fallback to OpenAI cl100k.
// TokenizerState is Send + Sync (both Tokenizer and CoreBPE are), and
// count() only requires &self, so no Mutex is needed — Lazy alone provides
// safe one-time initialization and lock-free concurrent reads.
static TOKENIZER: Lazy<TokenizerState> = Lazy::new(TokenizerState::new);

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
#[inline]
pub fn estimate_content_tokens(content: &str) -> usize {
    TOKENIZER.count(content)
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
}
