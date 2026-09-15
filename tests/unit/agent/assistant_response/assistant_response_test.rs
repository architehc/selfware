use super::{build_assistant_history_message, resolve_step_token_counts};

#[test]
fn uses_reported_values_when_present() {
    let (input, output) = resolve_step_token_counts(Some(1200), Some(300), 999, 999);
    assert_eq!(input, 1200);
    assert_eq!(output, 300);
}

#[test]
fn falls_back_to_estimates_when_usage_absent() {
    // Backend omitted usage entirely (common on local vLLM/SGLang).
    let (input, output) = resolve_step_token_counts(None, None, 1500, 220);
    assert_eq!(input, 1500);
    assert_eq!(output, 220);
}

#[test]
fn mixes_reported_and_estimated_per_field() {
    // Provider gave prompt tokens but not completion tokens.
    let (input, output) = resolve_step_token_counts(Some(1200), None, 999, 220);
    assert_eq!(input, 1200);
    assert_eq!(output, 220);
}

#[test]
fn test_assistant_response_reasoning_history_omitted_by_default() {
    let cfg = crate::config::Config::default();
    assert!(!cfg.preserve_thinking());

    let msg = build_assistant_history_message(
        "<think>internal thought</think>final answer",
        Some("internal thought".to_string()),
        None,
        cfg.preserve_thinking(),
    );
    assert_eq!(msg.content.text(), "final answer");
    assert!(
        msg.reasoning_content.is_none(),
        "history reasoning must be None when preserve_thinking=false"
    );
}

#[test]
fn test_assistant_response_reasoning_history_preserved_when_configured() {
    let mut cfg = crate::config::Config::default();
    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        serde_json::json!({ "preserve_thinking": true }),
    );
    cfg.extra_body = Some(extra);
    assert!(cfg.preserve_thinking());

    let msg = build_assistant_history_message(
        "<think>internal thought</think>final answer",
        Some("internal thought".to_string()),
        None,
        cfg.preserve_thinking(),
    );
    assert_eq!(msg.content.text(), "final answer");
    assert_eq!(
        msg.reasoning_content.as_deref(),
        Some("internal thought"),
        "history reasoning must be preserved when preserve_thinking=true"
    );
}

#[test]
fn test_multi_turn_context_growth_compact_with_preserve_thinking_false() {
    let mut messages = Vec::new();
    let cfg = crate::config::Config::default();

    // Turn 1
    messages.push(crate::api::types::Message::user("What is 2+2?"));
    let reasoning_1 = "The user is asking for 2+2. Let's compute it step by step...".repeat(50);
    messages.push(build_assistant_history_message(
        "4",
        Some(reasoning_1),
        None,
        cfg.preserve_thinking(),
    ));

    // Turn 2
    messages.push(crate::api::types::Message::user("What is 3+3?"));
    let reasoning_2 = "The user asks for 3+3...".repeat(50);
    messages.push(build_assistant_history_message(
        "6",
        Some(reasoning_2),
        None,
        cfg.preserve_thinking(),
    ));

    // Turn 3
    messages.push(crate::api::types::Message::user("What is 4+4?"));
    let reasoning_3 = "The user asks for 4+4...".repeat(50);
    messages.push(build_assistant_history_message(
        "8",
        Some(reasoning_3),
        None,
        cfg.preserve_thinking(),
    ));

    let total_chars: usize = messages
        .iter()
        .map(|m| m.content.len() + m.reasoning_content.as_ref().map_or(0, |r| r.len()))
        .sum();
    assert!(
        total_chars < 200,
        "history characters ({}) must stay small when preserve_thinking=false",
        total_chars
    );
    let estimated_tokens: usize = messages
        .iter()
        .map(|m| crate::token_count::estimate_content_tokens(m.content.text()))
        .sum();
    assert!(
        estimated_tokens < 100,
        "estimated history tokens ({}) should stay neat (<100)",
        estimated_tokens
    );
}
