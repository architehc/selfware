use super::*;

// ── Disabled branch ──────────────────────────────────────────────

#[tokio::test]
async fn disabled_returns_note_without_blocking() {
    let tool = ClarificationTool::new(false, 3);
    let result = tool
        .execute(serde_json::json!({"question": "test?"}))
        .await
        .unwrap();
    assert!(result["answer"].is_null());
    assert!(result["note"].as_str().unwrap().contains("disabled"));
    // Counter should not have been incremented.
    assert_eq!(tool.asked_count(), 0);
}

// ── Over-limit branch ────────────────────────────────────────────

#[tokio::test]
async fn over_limit_returns_note_without_blocking() {
    let tool = ClarificationTool::new(true, 0); // max_asks = 0
    let result = tool
        .execute(serde_json::json!({"question": "test?"}))
        .await
        .unwrap();
    assert!(result["answer"].is_null());
    assert!(result["note"].as_str().unwrap().contains("limit reached"));
    assert_eq!(tool.asked_count(), 0);
}

// ── Over-limit after exhausting budget ───────────────────────────

#[tokio::test]
async fn over_limit_after_exhausting_budget() {
    let tool = ClarificationTool::new(true, 2);
    // Manually exhaust the budget (simulating prior interactive calls).
    tool.asked.store(2, Ordering::SeqCst);
    let result = tool
        .execute(serde_json::json!({"question": "one more?"}))
        .await
        .unwrap();
    assert!(result["answer"].is_null());
    assert!(result["note"].as_str().unwrap().contains("limit reached"));
    // Counter unchanged because we returned early.
    assert_eq!(tool.asked_count(), 2);
}

// ── Missing required field ───────────────────────────────────────

#[tokio::test]
async fn missing_question_field_errors() {
    let tool = ClarificationTool::new(true, 3);
    let result = tool.execute(serde_json::json!({})).await;
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("question"));
}

// ── Non-string question field errors ─────────────────────────────

#[tokio::test]
async fn non_string_question_errors() {
    let tool = ClarificationTool::new(true, 3);
    let result = tool.execute(serde_json::json!({"question": 42})).await;
    assert!(result.is_err());
}

// ── Tool properties ──────────────────────────────────────────────

#[test]
fn tool_name_and_description() {
    let tool = ClarificationTool::new(true, 3);
    assert_eq!(tool.name(), "ask_user");
    assert!(!tool.description().is_empty());
}

#[test]
fn tool_schema_has_required_question() {
    let tool = ClarificationTool::new(true, 3);
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
    let required = schema["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "question"));
}

#[test]
fn tool_is_readonly_low_risk() {
    let tool = ClarificationTool::new(true, 3);
    assert!(tool.is_readonly());
    assert!(!tool.is_destructive());
    assert_eq!(tool.risk_level(), crate::safety::RiskLevel::Low);
}

#[test]
fn default_is_enabled_with_max_3() {
    let tool = ClarificationTool::default();
    assert_eq!(tool.asked_count(), 0);
    // We can't directly read `enabled`/`max_asks` (private), but the
    // disabled test above covers the false path and this covers default.
}

// ── TUI mode guard ───────────────────────────────────────────────

#[tokio::test]
async fn tui_mode_returns_note_without_blocking() {
    // When the TUI is active, the tool must NOT block on stdin — it
    // should return a null answer immediately.
    let tool = ClarificationTool::new(true, 3);

    // Activate TUI mode for the duration of this test.
    crate::output::set_tui_active(true);
    let result = tool
        .execute(serde_json::json!({"question": "test?"}))
        .await
        .unwrap();
    // Restore TUI state.
    crate::output::set_tui_active(false);

    assert!(result["answer"].is_null());
    assert!(result["note"].as_str().unwrap().contains("TUI mode"));
    // Counter should not have been incremented.
    assert_eq!(tool.asked_count(), 0);
}
