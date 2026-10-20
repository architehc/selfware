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
    let hyp_too_many = Hypothesis {
        id: "hyp-1".to_string(),
        description: "too many edits".to_string(),
        patch: r#"[
            {"file": "src/a.rs", "search": "a", "replace": "b"},
            {"file": "src/b.rs", "search": "c", "replace": "d"},
            {"file": "src/c.rs", "search": "e", "replace": "f"}
        ]"#
        .to_string(),
        target_files: vec![PathBuf::from("src/a.rs")],
        property_test: None,
    };
    assert!(validate_micro_hypothesis(&hyp_too_many).is_err());

    let hyp_protected_diff = Hypothesis {
        id: "hyp-2".to_string(),
        description: "protected unified diff".to_string(),
        patch: "--- a/src/safety/killswitch.rs\n+++ b/src/safety/killswitch.rs\n@@ -1,1 +1,1 @@\n-old\n+new\n"
            .to_string(),
        target_files: vec![],
        property_test: None,
    };
    assert!(validate_micro_hypothesis(&hyp_protected_diff).is_err());
}
