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
    assert!(
        count > 0,
        "fallback tokenizer must produce a positive count"
    );
}

#[test]
fn test_for_model_none_falls_back_gracefully() {
    // No model at all should still work — cl100k or heuristic.
    let state = TokenizerState::for_model(None);
    let count = state.count("hello world");
    assert!(count > 0);
}

#[test]
fn test_configured_model_slot_used_when_env_absent() {
    // The CLI registers the loaded config's model here instead of the
    // tokenizer reloading the config itself (P2-11).
    let _guard = crate::test_support::EnvGuard::clear_selfware_env();
    set_configured_model("slot-model");
    assert_eq!(configured_model_name().as_deref(), Some("slot-model"));
}

#[test]
fn test_env_var_beats_configured_model_slot() {
    let _guard = crate::test_support::EnvGuard::clear_selfware_env();
    set_configured_model("slot-model");
    std::env::set_var("SELFWARE_MODEL", "env-model");
    assert_eq!(configured_model_name().as_deref(), Some("env-model"));
    // The guard re-clears SELFWARE_MODEL on drop.
}

// ---- Ported from tests/unit/tokens (wave-3 D1): estimate_messages_tokens ----

#[test]
fn test_estimate_messages_tokens_simple() {
    use crate::api::types::Message;

    let messages = vec![
        Message::system("You are a helpful assistant"),
        Message::user("Hello, how are you?"),
        Message::assistant("I'm doing well, thank you!"),
    ];

    let estimate = estimate_messages_tokens(&messages);
    // At least 4 tokens overhead per message (3 messages) + content
    assert!(estimate > 12);
}

#[test]
fn test_estimate_messages_tokens_with_tool_calls() {
    use crate::api::types::{Message, ToolCall, ToolFunction};

    let mut msg = Message::assistant("Let me read that file for you.");
    msg.tool_calls = Some(vec![ToolCall {
        id: "call_1".to_string(),
        call_type: "function".to_string(),
        function: ToolFunction {
            name: "file_read".to_string(),
            arguments: r#"{"path": "test.txt"}"#.to_string(),
        },
    }]);

    let messages = vec![msg];
    let estimate = estimate_messages_tokens(&messages);

    // Should include tool call overhead
    assert!(estimate > 20);
}

#[test]
fn test_estimate_messages_tokens_empty() {
    let messages: Vec<crate::api::types::Message> = vec![];
    let estimate = estimate_messages_tokens(&messages);
    assert_eq!(estimate, 0);
}
