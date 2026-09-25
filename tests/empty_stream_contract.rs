//! The empty-stream contract, pinned.
//!
//! Measured against SGLang/qwen38-flash-next on 2026-09-12: a prompt the server
//! rejects with a clean HTTP 400 when non-streamed instead returns an empty SSE
//! stream with no error, no usage and no finish_reason. Sixteen concurrent
//! requests all "succeeded" and returned nothing.
//!
//! `StreamingResponse::collect()` deliberately still returns `Ok(empty)` — it is
//! a raw accumulator, and its empty-stream test came from a coverage sweep
//! (f1429d6c, "Add comprehensive test suite achieving 80%+ coverage"), not from
//! a decision that empty streams are valid completions. The contract lives one
//! level up, in `Agent::chat_streaming`.
//!
//! These tests pin the DISCRIMINATION, which is the part that matters: an
//! unexplained zero-token stream is an error, and every honest empty case is
//! not.

use selfware::errors::ApiError;

#[test]
fn an_unexplained_empty_stream_is_its_own_error() {
    let err = ApiError::EmptyStream;
    let text = err.to_string();
    assert!(
        text.contains("no content"),
        "the message must say what was missing: {text}"
    );
    assert!(
        text.contains("not a cancellation"),
        "the message must rule out the benign cause: {text}"
    );
}

#[test]
fn empty_stream_is_distinct_from_reasoning_starvation() {
    // Two zero-content failures with different signatures, measured on two
    // different models. Collapsing them loses the remedy: starvation is fixed
    // by raising max_tokens or lowering effort; an empty stream is not.
    //
    //   EmptyStream              reasoning=0    content=0  finish=None
    //   ReasoningBudgetExhausted reasoning=796  content=1  finish=length
    let empty = ApiError::EmptyStream.to_string();
    let starved = ApiError::ReasoningBudgetExhausted {
        reasoning_chars: 796,
        retry: None,
    }
    .to_string();
    assert_ne!(empty, starved);
    assert!(
        starved.contains("max_tokens") || starved.contains("reasoning effort"),
        "starvation must carry its remedy: {starved}"
    );
    assert!(
        !empty.contains("max_tokens"),
        "an empty stream must not suggest a budget fix that cannot help: {empty}"
    );
}

/// Both reasoning-pin call sites must agree, in both placements.
///
/// The client path and the profile path each carried their own copy of this
/// check. Fixing one fixed exactly one; a cross-distribution install test found
/// the profile path still blind to a nested pin. They now share a definition,
/// and this pins the behaviour that definition must have.
#[test]
fn a_nested_reasoning_pin_is_visible_to_every_caller() {
    use selfware::api::client::extra_body_pins_reasoning;
    use serde_json::{json, Map, Value};

    let obj = |v: Value| -> Map<String, Value> { v.as_object().unwrap().clone() };

    // Top-level, OpenAI spelling.
    assert!(extra_body_pins_reasoning(Some(&obj(
        json!({"reasoning_effort": "low"})
    ))));
    assert!(extra_body_pins_reasoning(Some(&obj(
        json!({"reasoning": {}})
    ))));

    // Nested, Qwen/SGLang chat-template spelling — the case that regressed.
    assert!(extra_body_pins_reasoning(Some(&obj(json!({
        "chat_template_kwargs": {"reasoning_effort": "low"}
    })))));
    assert!(extra_body_pins_reasoning(Some(&obj(json!({
        "chat_template_kwargs": {"enable_thinking": false}
    })))));

    // Unrelated settings are not a pin, and absence is not a pin.
    assert!(!extra_body_pins_reasoning(Some(&obj(json!({
        "chat_template_kwargs": {"add_vision_id": true}
    })))));
    assert!(!extra_body_pins_reasoning(Some(&obj(
        json!({"temperature": 0})
    ))));
    assert!(!extra_body_pins_reasoning(None));
}
