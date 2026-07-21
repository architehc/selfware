//! Per-turn debug artifacts.
//!
//! After every LLM call, selfware writes a JSON file under
//! `<workdir>/.selfware/turns/turn_NNNN.json` containing the sanitized
//! request body, the raw response body, the parsed tool calls, and the
//! agent's decision.  This makes post-mortem debugging of long runs
//! (NONTERM_PROSE failures, gate refusals, oscillation loops) tractable
//! without rerunning under multiple `SELFWARE_DEBUG_*` env vars.
//!
//! Capture is off by default. Set `agent.disable_turn_artifacts = false`
//! in `selfware.toml` (or env equivalent) to opt in for diagnostics.

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

/// Ensure a project-local `.selfware/` directory carries a `.gitignore` that
/// ignores everything inside it, so the agent's scratch (turn artifacts,
/// spilled tool results, …) can't be accidentally committed into the user's
/// repo. No-op if the dir doesn't exist or the file is already present.
/// Best-effort — a failure to write it must never break the caller.
pub(crate) fn ensure_selfware_gitignore(selfware_dir: &Path) {
    if !selfware_dir.is_dir() {
        return;
    }
    let gitignore = selfware_dir.join(".gitignore");
    if !gitignore.exists() {
        let _ = std::fs::write(&gitignore, "# Selfware scratch — do not commit.\n*\n");
    }
}

/// Maximum number of turn-artifact files to retain per workdir. Older files
/// are pruned so long-running or repeated sessions don't grow unbounded
/// (mirrors the checkpoint retention cap).
const MAX_TURN_ARTIFACTS: usize = 500;

/// Delete the oldest `turn_*.json` files in `dir` when their count exceeds
/// `MAX_TURN_ARTIFACTS`, ordered by last-modified time. Best-effort: any error
/// is logged and never propagated — pruning must never break the agent loop.
async fn prune_old_artifacts(dir: &Path) {
    let mut entries: Vec<(std::time::SystemTime, std::path::PathBuf)> = Vec::new();
    let mut rd = match tokio::fs::read_dir(dir).await {
        Ok(rd) => rd,
        Err(_) => return,
    };
    while let Ok(Some(entry)) = rd.next_entry().await {
        let path = entry.path();
        let is_turn = path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|n| n.starts_with("turn_") && n.ends_with(".json"))
            .unwrap_or(false);
        if !is_turn {
            continue;
        }
        let mtime = match entry.metadata().await.and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(_) => continue,
        };
        entries.push((mtime, path));
    }
    if entries.len() <= MAX_TURN_ARTIFACTS {
        return;
    }
    // Oldest first, then remove the overflow.
    entries.sort_by_key(|(t, _)| *t);
    let remove_count = entries.len() - MAX_TURN_ARTIFACTS;
    for (_, path) in entries.into_iter().take(remove_count) {
        if let Err(e) = tokio::fs::remove_file(&path).await {
            tracing::warn!("Failed to prune turn artifact {:?}: {}", path, e);
        }
    }
}

/// Write a `TurnArtifact` to `<workdir>/.selfware/turns/turn_{step:04}.json`.
///
/// Errors are logged but never propagated — debug capture must never break
/// the agent loop.
pub async fn write_artifact(workdir: &Path, artifact: &TurnArtifact) {
    let dir = artifact_dir(workdir);
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        tracing::warn!("Failed to create turn artifact dir {:?}: {}", dir, e);
        return;
    }
    // Drop a .gitignore into the project-local .selfware/ so scratch isn't
    // accidentally committed into the user's repo.
    ensure_selfware_gitignore(&workdir.join(".selfware"));
    let path = dir.join(format!("turn_{:04}.json", artifact.step));
    let json = match serde_json::to_string_pretty(artifact) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Failed to serialize turn artifact {}: {}", artifact.step, e);
            return;
        }
    };
    if let Err(e) = tokio::fs::write(&path, json).await {
        tracing::warn!("Failed to write turn artifact {:?}: {}", path, e);
        return;
    }
    prune_old_artifacts(&dir).await;
}

#[cfg(test)]
#[path = "../../tests/unit/agent/turn_artifacts/turn_artifacts_test.rs"]
mod tests;
