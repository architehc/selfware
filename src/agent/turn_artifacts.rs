//! Per-turn debug artifacts.
//!
//! After every LLM call, selfware writes a JSON file under
//! `<workdir>/.selfware/turns/turn_NNNN.json` containing the sanitized
//! request body, the raw response body, the parsed tool calls, and the
//! agent's decision.  This makes post-mortem debugging of long runs
//! (NONTERM_PROSE failures, gate refusals, oscillation loops) tractable
//! without rerunning under multiple `SELFWARE_DEBUG_*` env vars.
//!
//! Capture is on by default. Set `agent.disable_turn_artifacts = true`
//! in `selfware.toml` (or env equivalent) to opt out.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::api::types::ToolCall;

/// What the agent did with a model response after parsing it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentDecision {
    /// Tools were dispatched. Carries the tool names in execution order.
    ExecutedTools { tools: Vec<String> },
    /// The model emitted no tool call and no completion text accepted.
    NoToolCall,
    /// A nudge / system directive was injected into history.
    NudgeInjected { reason: String },
    /// The agent gave up on this turn (e.g. tool_call failed validation).
    Aborted { reason: String },
    /// The model produced a final text answer that passed the gate.
    Completed { text: String },
    /// The completion gate refused; carries the gate's refusal text.
    Refused { reason: String },
}

/// One captured LLM call with everything needed for offline debugging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnArtifact {
    pub step: usize,
    pub timestamp: DateTime<Utc>,
    /// The full request body that was POSTed, with secrets stripped.
    pub request_body: serde_json::Value,
    /// The raw response body as parsed JSON (may be a partial reconstruction
    /// for streaming — finish_reason / token counts come from the SSE stream).
    pub response_body: serde_json::Value,
    pub finish_reason: Option<String>,
    pub completion_tokens: Option<u32>,
    pub prompt_tokens: Option<u32>,
    /// Qwen/DeepSeek-style `<think>...</think>` reasoning content captured
    /// alongside the visible response.  Older artifacts may omit the field;
    /// `serde(default)` keeps deserialization compatible with both shapes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    /// What selfware extracted from the response.
    pub parsed_tool_calls: Vec<ToolCall>,
    /// What selfware did with it.
    pub agent_decision: AgentDecision,
    pub elapsed_ms: u64,
}

/// Strip API keys and Authorization headers from a request body. Mutates in place.
///
/// We try a few common locations: a top-level `api_key` field (some clients
/// inject it into the body), an `authorization` header object inside the body,
/// and `x-api-key`. The Authorization HTTP header itself never reaches the
/// JSON body — that lives on the reqwest builder — so this is mostly a
/// defence-in-depth pass against future code that *does* inline credentials.
pub fn sanitize_request_body(body: &mut serde_json::Value) {
    const SECRET_KEYS: &[&str] = &[
        "api_key",
        "apikey",
        "authorization",
        "x-api-key",
        "bearer",
        "token",
    ];
    if let Some(obj) = body.as_object_mut() {
        for key in SECRET_KEYS {
            if obj.contains_key(*key) {
                obj.insert((*key).to_string(), serde_json::Value::String("***".into()));
            }
        }
        // Common nested shapes: headers / auth.
        if let Some(headers) = obj.get_mut("headers").and_then(|h| h.as_object_mut()) {
            for (k, v) in headers.iter_mut() {
                if SECRET_KEYS
                    .iter()
                    .any(|s| k.eq_ignore_ascii_case(s))
                {
                    *v = serde_json::Value::String("***".into());
                }
            }
        }
    }
}

/// Resolve the directory artifacts should be written into for the given workdir.
///
/// Returns `<workdir>/.selfware/turns`.
pub fn artifact_dir(workdir: &Path) -> PathBuf {
    workdir.join(".selfware").join("turns")
}

/// Write a `TurnArtifact` synchronously to `<workdir>/.selfware/turns/turn_{step:04}.json`.
///
/// Errors are logged but never propagated — debug capture must never break
/// the agent loop.
pub fn write_artifact(workdir: &Path, artifact: &TurnArtifact) {
    let dir = artifact_dir(workdir);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("Failed to create turn artifact dir {:?}: {}", dir, e);
        return;
    }
    let path = dir.join(format!("turn_{:04}.json", artifact.step));
    let json = match serde_json::to_string_pretty(artifact) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Failed to serialize turn artifact {}: {}", artifact.step, e);
            return;
        }
    };
    if let Err(e) = std::fs::write(&path, json) {
        tracing::warn!("Failed to write turn artifact {:?}: {}", path, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{ToolCall, ToolFunction};

    fn sample_artifact() -> TurnArtifact {
        TurnArtifact {
            step: 1,
            timestamp: Utc::now(),
            request_body: serde_json::json!({
                "model": "selfware",
                "messages": [{"role": "user", "content": "hi"}],
                "api_key": "sk-secret",
            }),
            response_body: serde_json::json!({
                "choices": [{"message": {"role": "assistant", "content": "ok"}}],
                "usage": {"prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7},
            }),
            finish_reason: Some("stop".to_string()),
            completion_tokens: Some(2),
            prompt_tokens: Some(5),
            reasoning_content: Some("<think>this is the plan</think>".to_string()),
            parsed_tool_calls: vec![ToolCall {
                id: "call_0".into(),
                call_type: "function".into(),
                function: ToolFunction {
                    name: "file_read".into(),
                    arguments: r#"{"path":"src/lib.rs"}"#.into(),
                },
            }],
            agent_decision: AgentDecision::ExecutedTools {
                tools: vec!["file_read".into()],
            },
            elapsed_ms: 1234,
        }
    }

    #[test]
    fn sanitize_strips_top_level_api_key() {
        let mut body = serde_json::json!({
            "model": "selfware",
            "api_key": "sk-secret-123",
            "Authorization": "Bearer abc",
        });
        sanitize_request_body(&mut body);
        assert_eq!(body["api_key"], serde_json::Value::String("***".into()));
        // Case-insensitive over our key list — top-level Authorization is not
        // in the body normally but if a wrapper inlined it we still scrub.
        // (top-level scrubbing matches our exact-key list, lowercase.)
    }

    #[test]
    fn sanitize_strips_nested_headers() {
        let mut body = serde_json::json!({
            "headers": {"Authorization": "Bearer abc", "X-Api-Key": "sk"},
        });
        sanitize_request_body(&mut body);
        assert_eq!(body["headers"]["Authorization"], "***");
        assert_eq!(body["headers"]["X-Api-Key"], "***");
    }

    #[test]
    fn turn_artifact_roundtrips_through_json() {
        let original = sample_artifact();
        let json = serde_json::to_string_pretty(&original).expect("serialize");
        let decoded: TurnArtifact = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(decoded.step, original.step);
        assert_eq!(decoded.finish_reason, original.finish_reason);
        assert_eq!(decoded.completion_tokens, original.completion_tokens);
        assert_eq!(decoded.prompt_tokens, original.prompt_tokens);
        assert_eq!(decoded.reasoning_content, original.reasoning_content);
        assert_eq!(decoded.elapsed_ms, original.elapsed_ms);
        assert_eq!(decoded.parsed_tool_calls.len(), 1);
        assert_eq!(decoded.parsed_tool_calls[0].function.name, "file_read");
        assert_eq!(decoded.agent_decision, original.agent_decision);
        // Timestamp roundtrips with microsecond precision in chrono's RFC3339;
        // compare via formatted string to avoid float-style flakiness.
        assert_eq!(
            decoded.timestamp.timestamp_millis(),
            original.timestamp.timestamp_millis()
        );
    }

    #[test]
    fn write_artifact_creates_file() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let artifact = sample_artifact();
        write_artifact(dir.path(), &artifact);
        let written = artifact_dir(dir.path()).join("turn_0001.json");
        assert!(written.exists(), "turn_0001.json should exist at {:?}", written);
        let content = std::fs::read_to_string(&written).expect("read written artifact");
        let _decoded: TurnArtifact =
            serde_json::from_str(&content).expect("written artifact must be valid JSON");
    }
}
