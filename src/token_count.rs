//! Shared token counting utilities.
//!
//! Uses `tiktoken-rs` for accurate counts and falls back to a conservative
//! heuristic if tokenizer initialization fails.

use once_cell::sync::Lazy;
use tiktoken_rs::{cl100k_base, CoreBPE};

static TOKENIZER: Lazy<Option<CoreBPE>> = Lazy::new(|| cl100k_base().ok());

/// Estimate token count for content and add a fixed per-message overhead.
#[inline]
pub fn estimate_tokens_with_overhead(content: &str, message_overhead: usize) -> usize {
    estimate_content_tokens(content) + message_overhead
}

/// Estimate tokens for raw content.
#[inline]
pub fn estimate_content_tokens(content: &str) -> usize {
    TOKENIZER
        .as_ref()
        .map(|bpe| bpe.encode_with_special_tokens(content).len())
        .unwrap_or_else(|| heuristic_estimate(content))
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
