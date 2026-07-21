use super::*;

#[test]
fn test_token_tracker_new() {
    let tracker = TokenTracker::new();
    assert_eq!(tracker.total_tokens(), 0);
    assert_eq!(tracker.api_call_count(), 0);
}

#[test]
fn test_record_usage() {
    let tracker = TokenTracker::new();
    tracker.record_usage(100, 50);

    assert_eq!(tracker.total_prompt_tokens(), 100);
    assert_eq!(tracker.total_completion_tokens(), 50);
    assert_eq!(tracker.total_tokens(), 150);
    assert_eq!(tracker.api_call_count(), 1);
}

#[test]
fn test_record_multiple() {
    let tracker = TokenTracker::new();
    tracker.record_usage(100, 50);
    tracker.record_usage(200, 100);

    assert_eq!(tracker.total_prompt_tokens(), 300);
    assert_eq!(tracker.total_completion_tokens(), 150);
    assert_eq!(tracker.api_call_count(), 2);
}

#[test]
fn test_record_step() {
    let tracker = TokenTracker::new();
    tracker.record_step(1, 100, 50, Some("file_read".to_string()));
    tracker.record_step(2, 150, 75, Some("shell_exec".to_string()));

    let steps = tracker.step_usage();
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0].step, 1);
    assert_eq!(steps[0].tool_name, Some("file_read".to_string()));
}

#[test]
fn test_estimate_cost() {
    let tracker = TokenTracker::new();
    tracker.record_usage(1_000_000, 100_000);

    let cost = tracker.estimate_cost();
    // With no model_id set, uses sonnet pricing as default:
    // 1M prompt tokens at $0.003/1K + 100K completion at $0.015/1K
    // = $3 + $1.5 = $4.5
    assert!((cost - 4.5).abs() < 0.01);
}

#[test]
fn test_estimate_cost_with_haiku_model() {
    let tracker = TokenTracker::new();
    tracker.set_model_id("claude-3-haiku");
    tracker.record_usage(1_000_000, 100_000);

    let cost = tracker.estimate_cost();
    // Haiku pricing: $0.00025/1K input, $0.00125/1K output
    // 1M/1000 * 0.00025 + 100K/1000 * 0.00125
    // = 1000 * 0.00025 + 100 * 0.00125
    // = 0.25 + 0.125 = 0.375
    assert!((cost - 0.375).abs() < 0.001);
}

#[test]
fn test_estimate_cost_with_sonnet_model() {
    let tracker = TokenTracker::new();
    tracker.set_model_id("claude-3-5-sonnet");
    tracker.record_usage(1_000_000, 100_000);

    let cost = tracker.estimate_cost();
    // Sonnet pricing: $0.003/1K input, $0.015/1K output
    // 1M/1000 * 0.003 + 100K/1000 * 0.015
    // = 1000 * 0.003 + 100 * 0.015
    // = $3 + $1.5 = $4.5
    assert!((cost - 4.5).abs() < 0.01);
}

#[test]
fn test_estimate_cost_with_opus_model() {
    let tracker = TokenTracker::new();
    tracker.set_model_id("claude-3-opus");
    tracker.record_usage(1_000_000, 100_000);

    let cost = tracker.estimate_cost();
    // Opus pricing: $0.015/1K input, $0.075/1K output
    // 1M/1000 * 0.015 + 100K/1000 * 0.075
    // = 1000 * 0.015 + 100 * 0.075
    // = $15 + $7.5 = $22.5
    assert!((cost - 22.5).abs() < 0.01);
}

#[test]
fn test_estimate_cost_unknown_model_fallback() {
    let tracker = TokenTracker::new();
    // Unknown model should fall back to sonnet pricing
    tracker.set_model_id("unknown-model-v1");
    tracker.record_usage(1_000_000, 100_000);

    let cost = tracker.estimate_cost();
    // Should use sonnet pricing as fallback
    assert!((cost - 4.5).abs() < 0.01);
    // ...but the fallback must be LABELLED, not silent: the summary
    // shows whose rates were actually applied.
    let basis = tracker.cost_basis_label();
    assert!(
        basis.contains("unknown rates for 'unknown-model-v1'"),
        "fallback must be labelled, got: {}",
        basis
    );
    let display = format!("{}", tracker.summary());
    assert!(
        display.contains("claude-3-5-sonnet estimate"),
        "summary display must carry the honest basis: {}",
        display
    );
}

#[test]
fn test_estimate_cost_with_glm_5_2_model() {
    let tracker = TokenTracker::new();
    // The shipped default: z-ai/glm-5.2 via OpenRouter.
    tracker.set_model_id("z-ai/glm-5.2");
    tracker.record_usage(1_000_000, 100_000);

    let cost = tracker.estimate_cost();
    // GLM-5.2 pricing: $0.0014/1K input, $0.0044/1K output
    // = 1000 * 0.0014 + 100 * 0.0044 = $1.4 + $0.44 = $1.84
    assert!(
        (cost - 1.84).abs() < 0.01,
        "GLM-5.2 should use its own rates, not sonnet's; got {}",
        cost
    );
    assert_eq!(tracker.cost_basis_label(), "rates for z-ai/glm-5.2");
}

#[test]
fn test_known_pricing_for_model_map() {
    // Substring, case-insensitive matching over the small built-in map.
    assert_eq!(
        known_pricing_for_model("z-ai/glm-5.2").map(|p| p.model_id),
        Some("z-ai/glm-5.2".to_string())
    );
    assert_eq!(
        known_pricing_for_model("z-ai/glm-5.2-20260616").map(|p| p.model_id),
        Some("z-ai/glm-5.2".to_string())
    );
    assert_eq!(
        known_pricing_for_model("Claude-3-5-Sonnet").map(|p| p.model_id),
        Some("claude-3-5-sonnet".to_string())
    );
    // Unknown models get NO rate card — never silently sonnet.
    assert!(known_pricing_for_model("glm-4.6").is_none());
    assert!(known_pricing_for_model("gpt-whatever").is_none());
    assert!(known_pricing_for_model("qwen3.6-27b").is_none());
}

#[test]
fn test_cost_basis_label_unset_model_is_honest() {
    let tracker = TokenTracker::new();
    assert_eq!(
        tracker.cost_basis_label(),
        "no model set — claude-3-5-sonnet estimate"
    );
}

#[test]
fn test_set_and_get_model_id() {
    let tracker = TokenTracker::new();
    assert_eq!(tracker.get_model_id(), None);

    tracker.set_model_id("claude-3-opus");
    assert_eq!(tracker.get_model_id(), Some("claude-3-opus".to_string()));

    tracker.set_model_id("claude-3-haiku");
    assert_eq!(tracker.get_model_id(), Some("claude-3-haiku".to_string()));
}

#[test]
fn test_model_id_preserved_on_reset() {
    let tracker = TokenTracker::new();
    tracker.set_model_id("claude-3-opus");
    tracker.record_usage(1000, 500);

    tracker.reset();

    // Model ID should be preserved after reset
    assert_eq!(tracker.get_model_id(), Some("claude-3-opus".to_string()));
    // But tokens should be reset
    assert_eq!(tracker.total_tokens(), 0);
}

#[test]
fn test_reset() {
    let tracker = TokenTracker::new();
    tracker.record_usage(100, 50);
    tracker.reset();

    assert_eq!(tracker.total_tokens(), 0);
    assert_eq!(tracker.api_call_count(), 0);
}

#[test]
fn test_summary_display() {
    let tracker = TokenTracker::new();
    tracker.record_usage(1000, 500);

    let summary = tracker.summary();
    let display = format!("{}", summary);

    assert!(display.contains("1500"));
    assert!(display.contains("1000"));
    assert!(display.contains("500"));
}

#[test]
fn test_estimate_tokens_short() {
    let estimate = estimate_tokens("Hello, world!");
    assert!(estimate > 0);
    assert!(estimate < 10);
}

#[test]
fn test_estimate_tokens_long() {
    let text = "This is a longer piece of text that should result in more tokens being estimated.";
    let estimate = estimate_tokens(text);
    assert!(estimate > 10);
}

#[test]
fn test_estimate_tokens_code() {
    let code = r#"
fn main() {
    println!("Hello, world!");
}
"#;
    let estimate = estimate_tokens(code);
    assert!(estimate > 5);
}

#[test]
fn test_estimate_json_tokens() {
    let json = serde_json::json!({
        "name": "test",
        "values": [1, 2, 3],
        "nested": {"a": 1, "b": 2}
    });

    let estimate = estimate_json_tokens(&json);
    // Small JSON objects produce ~5-10 tokens
    assert!(estimate > 5);
}

#[test]
fn test_session_duration() {
    let tracker = TokenTracker::new();
    std::thread::sleep(std::time::Duration::from_millis(10));

    let duration = tracker.session_duration();
    assert!(duration.is_some());
    assert!(duration.unwrap().as_millis() >= 10);
}

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

// ---- Drift tracking tests ----

#[test]
fn test_drift_stats_default() {
    let tracker = TokenTracker::new();
    let drift = tracker.drift_stats();
    assert_eq!(drift.samples, 0);
    assert_eq!(drift.cumulative_drift, 0);
}

#[test]
fn test_drift_over_estimate() {
    let tracker = TokenTracker::new();
    // Estimated 120, actual 100 → over-estimate by 20
    tracker.record_drift(120, 100);
    let drift = tracker.drift_stats();
    assert_eq!(drift.samples, 1);
    assert_eq!(drift.cumulative_drift, 20);
    assert_eq!(drift.cumulative_abs_drift, 20);
    assert_eq!(drift.max_over, 20);
    assert_eq!(drift.max_under, 0);
}

#[test]
fn test_drift_under_estimate() {
    let tracker = TokenTracker::new();
    // Estimated 80, actual 100 → under-estimate by 20
    tracker.record_drift(80, 100);
    let drift = tracker.drift_stats();
    assert_eq!(drift.samples, 1);
    assert_eq!(drift.cumulative_drift, -20);
    assert_eq!(drift.cumulative_abs_drift, 20);
    assert_eq!(drift.max_over, 0);
    assert_eq!(drift.max_under, -20);
}

#[test]
fn test_drift_accumulation() {
    let tracker = TokenTracker::new();
    tracker.record_drift(110, 100); // +10
    tracker.record_drift(90, 100); // -10
    tracker.record_drift(130, 100); // +30
    let drift = tracker.drift_stats();
    assert_eq!(drift.samples, 3);
    assert_eq!(drift.cumulative_drift, 30); // 10 + (-10) + 30
    assert_eq!(drift.cumulative_abs_drift, 50); // 10 + 10 + 30
    assert_eq!(drift.max_over, 30);
    assert_eq!(drift.max_under, -10);
}

#[test]
fn test_drift_reset() {
    let tracker = TokenTracker::new();
    tracker.record_drift(150, 100);
    tracker.reset();
    let drift = tracker.drift_stats();
    assert_eq!(drift.samples, 0);
    assert_eq!(drift.cumulative_drift, 0);
}

#[test]
fn test_drift_in_summary() {
    let tracker = TokenTracker::new();
    tracker.record_usage(1000, 500);
    tracker.record_drift(1100, 1000);
    let summary = tracker.summary();
    assert_eq!(summary.drift.samples, 1);
    let display = format!("{}", summary);
    assert!(display.contains("Drift"));
}
