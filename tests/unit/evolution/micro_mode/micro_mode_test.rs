use super::*;

#[test]
fn test_is_micro_model() {
    // Positive: known micro sizes
    assert!(is_micro_model("qwen3.5-0.8b"));
    assert!(is_micro_model("Qwen3.5-1.5B-Instruct"));
    assert!(is_micro_model("Llama-3.2-1B"));
    assert!(is_micro_model("phi-2b"));
    assert!(is_micro_model("gemma-3b"));
    assert!(is_micro_model("tiny-llama"));
    assert!(is_micro_model("SmolLM-small"));
    assert!(is_micro_model("model-4b-instruct"));

    // Negative: larger models must NOT match
    assert!(!is_micro_model("qwen3.5-32b"));
    assert!(!is_micro_model("qwen3.5-27b"));
    assert!(!is_micro_model("Llama-3.2-13b"));
    assert!(!is_micro_model("Llama-3.1-12b"));
    assert!(!is_micro_model("Qwen-23b"));
    assert!(!is_micro_model("gpt-4"));
    assert!(!is_micro_model("claude-opus"));
}

#[test]
fn test_build_micro_system_prompt() {
    let prompt = build_micro_system_prompt(5);
    assert!(prompt.contains("Generate 3")); // Clamped to 3
    assert!(prompt.contains("JSON array"));
    assert!(prompt.contains("/no_think"));
}

#[test]
fn test_validate_micro_hypothesis_too_many_edits() {
    // This would need a real Hypothesis with JSON patch
    // Skipping for now - would need mock data
}
