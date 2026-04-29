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

/// Sentinel that replaces redacted secret values in artifact files.
const REDACTED: &str = "<redacted>";

/// Returns `true` if `key` looks like a credential field name.
///
/// Matches (case-insensitive):
/// - `authorization`
/// - `bearer`
/// - `secret`
/// - `password`
/// - any key containing `api_key`, `apikey`, or `api-key` as a substring
///   (catches `api_key`, `apiKey`, `x-api-key`, `openai_api_key`, …)
/// - keys whose normalized form ends in `token` (e.g. `token`, `access_token`,
///   `auth-token`) but NOT `tool_call_id`, `completion_tokens`,
///   `prompt_tokens`, `total_tokens`, `max_tokens`, etc.
fn key_is_secret(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "authorization" | "bearer" | "secret" | "password"
    ) {
        return true;
    }
    // `api_key`, `apikey`, `api-key`, `x-api-key`, `openai_api_key`, …
    if lower.contains("api_key") || lower.contains("apikey") || lower.contains("api-key") {
        return true;
    }
    // Token suffix matching: normalize separators so `auth-token` and
    // `auth_token` both match. We require an exact `token` suffix on a
    // word boundary, so `*_token` / `*-token` / bare `token` match but
    // `tokens`, `tokenizer`, `completion_tokens`, `prompt_tokens`,
    // `total_tokens`, `max_tokens`, `tool_call_id` don't.
    let norm = lower.replace('-', "_");
    if norm == "token" {
        return true;
    }
    if let Some(stripped) = norm.strip_suffix("_token") {
        // Defensive: the stripped prefix must be non-empty.
        if !stripped.is_empty() {
            return true;
        }
    }
    false
}

/// Strip API keys, Authorization headers, and bearer tokens from a request body.
/// Mutates in place, walking the entire JSON tree to any depth.
///
/// Sanitization is defence-in-depth — the HTTP `Authorization` header never
/// reaches the request body for our own client (it's set on the reqwest
/// builder).  But OpenAI-compatible backends, custom `extra_body` shapes, and
/// future wrappers can and do inline credentials in nested fields like
/// `extra_body.api_key`, `headers.X-API-KEY`, `auth.bearer_token`.  Walking
/// recursively keeps the persistent per-turn artifacts under
/// `<workdir>/.selfware/turns/` from leaking those.
///
/// Matched keys are replaced with the literal string `"<redacted>"`.  Values
/// that happen to be objects/arrays are still descended into first, so a
/// nested credential under a non-secret-named key is still scrubbed.
pub fn sanitize_request_body(body: &mut serde_json::Value) {
    sanitize_value(body);
}

/// Recursive walker for [`sanitize_request_body`].
fn sanitize_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if key_is_secret(k) {
                    *v = serde_json::Value::String(REDACTED.to_string());
                } else {
                    sanitize_value(v);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for v in items.iter_mut() {
                sanitize_value(v);
            }
        }
        _ => {}
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
    fn sanitize_strips_top_level_api_key_and_authorization() {
        let mut body = serde_json::json!({
            "model": "selfware",
            "api_key": "sk-secret-123",
            "Authorization": "Bearer abc",
        });
        sanitize_request_body(&mut body);
        assert_eq!(body["api_key"], "<redacted>");
        assert_eq!(body["Authorization"], "<redacted>");
        // Non-secret keys preserved.
        assert_eq!(body["model"], "selfware");
    }

    #[test]
    fn sanitize_strips_nested_headers_with_mixed_case() {
        let mut body = serde_json::json!({
            "headers": {
                "Authorization": "Bearer abc",
                "X-Api-Key": "sk",
                "AUTHORIZATION": "Bearer XYZ",
                "X-API-KEY": "sk-upper",
                "Content-Type": "application/json",
            },
        });
        sanitize_request_body(&mut body);
        assert_eq!(body["headers"]["Authorization"], "<redacted>");
        assert_eq!(body["headers"]["X-Api-Key"], "<redacted>");
        assert_eq!(body["headers"]["AUTHORIZATION"], "<redacted>");
        assert_eq!(body["headers"]["X-API-KEY"], "<redacted>");
        // Non-secret header preserved.
        assert_eq!(body["headers"]["Content-Type"], "application/json");
    }

    #[test]
    fn sanitize_walks_deeply_nested_extra_body() {
        // Real-world shape: `extra_body.headers.api_key`, plus `extra_body.api_key`
        // (some OpenAI-compatible backends accept this), plus `auth.bearer_token`.
        let mut body = serde_json::json!({
            "model": "selfware",
            "extra_body": {
                "api_key": "sk-extra-body-leak",
                "headers": {
                    "Authorization": "Bearer deep",
                    "X-API-KEY": "sk-deep",
                },
                "auth": {
                    "bearer_token": "tok-1",
                    "access_token": "tok-2",
                },
            },
        });
        sanitize_request_body(&mut body);
        assert_eq!(body["extra_body"]["api_key"], "<redacted>");
        assert_eq!(body["extra_body"]["headers"]["Authorization"], "<redacted>");
        assert_eq!(body["extra_body"]["headers"]["X-API-KEY"], "<redacted>");
        assert_eq!(body["extra_body"]["auth"]["bearer_token"], "<redacted>");
        assert_eq!(body["extra_body"]["auth"]["access_token"], "<redacted>");
        // Sibling field preserved.
        assert_eq!(body["model"], "selfware");
    }

    #[test]
    fn sanitize_handles_arrays_and_prefix_variants() {
        // Authorization headers buried inside a `tools[].config.headers` array
        // and a top-level `secrets[]` array of credential objects.
        let mut body = serde_json::json!({
            "tools": [
                {"name": "fetch", "config": {"headers": {"authorization": "Bearer leak1"}}},
                {"name": "post",  "config": {"headers": {"X-Api-Key": "leak2"}}},
            ],
            "secrets": [
                {"password": "p1", "api_key": "k1"},
                {"refresh_token": "rt1"},
            ],
            "openai_api_key": "sk-inline",
        });
        sanitize_request_body(&mut body);
        assert_eq!(
            body["tools"][0]["config"]["headers"]["authorization"],
            "<redacted>"
        );
        assert_eq!(
            body["tools"][1]["config"]["headers"]["X-Api-Key"],
            "<redacted>"
        );
        assert_eq!(body["secrets"][0]["password"], "<redacted>");
        assert_eq!(body["secrets"][0]["api_key"], "<redacted>");
        assert_eq!(body["secrets"][1]["refresh_token"], "<redacted>");
        assert_eq!(body["openai_api_key"], "<redacted>");
    }

    #[test]
    fn sanitize_preserves_token_count_fields() {
        // The `token` matcher must not eat `completion_tokens`, `prompt_tokens`,
        // `total_tokens`, `max_tokens`, or `tool_call_id`.  These are usage /
        // identifier fields that we explicitly want preserved in artifacts.
        let mut body = serde_json::json!({
            "messages": [{
                "role": "tool",
                "tool_call_id": "call_123",
                "content": "ok",
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15,
            },
            "max_tokens": 4096,
            "tokenizer": "cl100k",
        });
        sanitize_request_body(&mut body);
        assert_eq!(body["messages"][0]["tool_call_id"], "call_123");
        assert_eq!(body["usage"]["prompt_tokens"], 10);
        assert_eq!(body["usage"]["completion_tokens"], 5);
        assert_eq!(body["usage"]["total_tokens"], 15);
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(body["tokenizer"], "cl100k");
    }

    #[test]
    fn sanitize_redacts_token_suffix_keys() {
        // `*_token` and bare `token` should be redacted.
        let mut body = serde_json::json!({
            "token": "raw",
            "access_token": "at",
            "refresh_token": "rt",
            "id_token": "idt",
            "auth-token": "ath",
        });
        sanitize_request_body(&mut body);
        assert_eq!(body["token"], "<redacted>");
        assert_eq!(body["access_token"], "<redacted>");
        assert_eq!(body["refresh_token"], "<redacted>");
        assert_eq!(body["id_token"], "<redacted>");
        assert_eq!(body["auth-token"], "<redacted>");
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
        assert!(
            written.exists(),
            "turn_0001.json should exist at {:?}",
            written
        );
        let content = std::fs::read_to_string(&written).expect("read written artifact");
        let _decoded: TurnArtifact =
            serde_json::from_str(&content).expect("written artifact must be valid JSON");
    }
}
