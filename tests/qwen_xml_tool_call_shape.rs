//! Regression fixture: the tool-call bytes `llm.selfware.design` actually returns.
//!
//! On 2026-09-12 that endpoint answered a `tools`-bearing request with
//! `finish_reason: "tool_calls"`, an EMPTY `tool_calls` array, and the call
//! itself as text in `content`. Selfware absorbs this through the unconditional
//! XML fallback in `api::tool_calling::extract_tool_calls`.
//!
//! These assertions pin the recovered call precisely — an earlier version of
//! this test asserted only that *one* call was parsed, which would have passed
//! with the wrong function name and both arguments lost.
//!
//! Scope: this covers parsing only. Whether a full agentic round trip works
//! against that endpoint (execute, return the result, continue coherently) is a
//! separate and currently unverified claim.

use selfware::api::tool_calling::extract_tool_calls;
use selfware::api::types::Message;

/// Byte-for-byte as returned by the live endpoint.
const LIVE_XML: &str = "<tool_call>\n<function=calculator>\n<parameter=a>\n17\n</parameter>\n<parameter=b>\n23\n</parameter>\n</function>\n</tool_call>";

#[test]
fn live_qwen_xml_parses_to_the_exact_call() {
    let parsed = selfware::tool_parser::parse_tool_calls(LIVE_XML);
    assert_eq!(parsed.tool_calls.len(), 1, "expected exactly one call");
    let call = &parsed.tool_calls[0];
    assert_eq!(call.tool_name, "calculator", "wrong function recovered");
    assert_eq!(
        call.arguments["a"], 17,
        "argument `a` lost or altered: {:?}",
        call.arguments
    );
    assert_eq!(
        call.arguments["b"], 23,
        "argument `b` lost or altered: {:?}",
        call.arguments
    );
}

#[test]
fn the_unified_extractor_recovers_it_when_tool_calls_is_empty() {
    // The shape that matters: a provider that sets finish_reason "tool_calls"
    // but leaves the array empty. `native_function_calling` is true here on
    // purpose — the fallback must not be gated on it.
    let message = Message::assistant(LIVE_XML);
    let calls = extract_tool_calls(&message, true);
    assert_eq!(calls.len(), 1, "unified extractor recovered nothing");
    assert_eq!(calls[0].function.name, "calculator");

    let args: serde_json::Value = serde_json::from_str(&calls[0].function.arguments)
        .expect("recovered arguments must be valid JSON");
    assert_eq!(args["a"], 17, "argument `a` lost: {args:?}");
    assert_eq!(args["b"], 23, "argument `b` lost: {args:?}");
}

#[test]
fn a_native_tool_call_is_preferred_over_text() {
    // Guard the other direction: the fallback must not shadow a real call.
    let mut message = Message::assistant(LIVE_XML);
    message.tool_calls = Some(vec![selfware::api::types::ToolCall {
        id: "call_native".to_string(),
        call_type: "function".to_string(),
        function: selfware::api::types::ToolFunction {
            name: "native_fn".to_string(),
            arguments: "{\"x\":1}".to_string(),
        },
    }]);
    let calls = extract_tool_calls(&message, true);
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].function.name, "native_fn",
        "a structured call must win over text parsing"
    );
}
