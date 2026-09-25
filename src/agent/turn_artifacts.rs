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
///
/// The decision must name what actually happened (AGENTS.md rule 3): a turn is
/// `executed_tools` only for the calls that reached execution, calls rejected
/// before execution are listed with the reason, and a turn whose batch never
/// reached dispatch says so instead of claiming execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentDecision {
    /// Tool calls were parsed; whether any of them reaches execution is not
    /// yet known. Recorded when a turn is captured before dispatch and
    /// refined once dispatch has happened.
    ///
    /// The Rust name is kept for source compatibility with existing call
    /// sites; it serializes as `pending_dispatch` because no execution has
    /// been observed when it is recorded.
    #[serde(rename = "pending_dispatch")]
    ExecutedTools { tools: Vec<String> },
    /// The batch was dispatched. `tools` lists ONLY the calls that reached
    /// execution, in order, each with its outcome; calls the dispatcher
    /// refused before execution (unknown tool, schema validation, safety
    /// block, policy, duplicate suppression, ...) are in `rejected_tools`.
    #[serde(rename = "executed_tools")]
    Dispatched {
        tools: Vec<ExecutedTool>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        rejected_tools: Vec<RejectedTool>,
    },
    /// Every parsed call was refused before execution; nothing ran.
    RejectedTools { rejected_tools: Vec<RejectedTool> },
    /// A gate stopped the turn before the batch was dispatched (budget cap,
    /// plan mode, repetition guard, progress guard, cancellation, ...).
    /// `tools` names the parsed calls that did not run.
    StoppedBeforeDispatch { reason: String, tools: Vec<String> },
    /// The model emitted no tool call and no completion text accepted.
    NoToolCall,
    /// A nudge / system directive was injected into history.
    NudgeInjected { reason: String },
    /// The agent gave up on this turn (e.g. tool_call failed validation).
    Aborted { reason: String },
    /// The model produced a final text answer that was accepted.
    #[serde(alias = "completed")]
    FinalAnswer { text: String },
    /// The completion gate refused; carries the gate's refusal text.
    Refused { reason: String },
}

/// A tool call that reached execution, with its outcome.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutedTool {
    pub name: String,
    /// Whether the tool reported success.
    pub ok: bool,
}

/// A tool call refused before execution, with the refusal the model saw.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RejectedTool {
    pub name: String,
    pub reason: String,
}

/// One dispatcher event for a tool call of the current batch, in order.
/// Recorded by the dispatch funnels; [`classify_dispatch`] turns the journal
/// into the turn's [`AgentDecision`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DispatchEvent {
    /// The call reached execution (the tool ran, or was served from the
    /// tool cache inside the execution path).
    Executed { name: String, ok: bool },
    /// A tool-result (or skip) message was pushed for the call. Paired with
    /// a preceding unconsumed `Executed` of the same name it is that
    /// execution's result; otherwise the call was refused before execution
    /// and `text` is the refusal.
    Answered {
        name: String,
        success: bool,
        text: String,
    },
}

/// Longest refusal reason kept in an artifact.
const MAX_REJECTION_REASON_CHARS: usize = 300;

/// Classify a dispatched batch from its dispatcher journal.
///
/// `parsed` are the tool names the batch was called with, in order. A call
/// that was neither executed nor answered (e.g. cancellation broke the loop)
/// is reported as rejected with `unanswered_reason`. With nothing executed
/// and nothing refused, the batch was stopped before dispatch.
pub(crate) fn classify_dispatch(
    parsed: &[String],
    journal: &[DispatchEvent],
    unanswered_reason: &str,
) -> AgentDecision {
    let mut executed: Vec<ExecutedTool> = Vec::new();
    let mut rejected: Vec<RejectedTool> = Vec::new();
    // Executions not yet paired with their result message.
    let mut open: Vec<usize> = Vec::new();
    for event in journal {
        match event {
            DispatchEvent::Executed { name, ok } => {
                executed.push(ExecutedTool {
                    name: name.clone(),
                    ok: *ok,
                });
                open.push(executed.len() - 1);
            }
            DispatchEvent::Answered {
                name,
                success,
                text,
            } => {
                if let Some(pos) = open.iter().position(|&i| executed[i].name == *name) {
                    open.remove(pos);
                } else if !*success {
                    rejected.push(RejectedTool {
                        name: name.clone(),
                        reason: text.chars().take(MAX_REJECTION_REASON_CHARS).collect(),
                    });
                }
            }
        }
    }
    if executed.is_empty() && rejected.is_empty() {
        return AgentDecision::StoppedBeforeDispatch {
            reason: unanswered_reason.to_string(),
            tools: parsed.to_vec(),
        };
    }
    // Parsed calls neither executed nor refused never got a result.
    let mut accounted: Vec<&str> = executed
        .iter()
        .map(|t| t.name.as_str())
        .chain(rejected.iter().map(|t| t.name.as_str()))
        .collect();
    let mut unanswered = Vec::new();
    for name in parsed {
        if let Some(pos) = accounted.iter().position(|n| *n == name.as_str()) {
            accounted.remove(pos);
        } else {
            unanswered.push(RejectedTool {
                name: name.clone(),
                reason: unanswered_reason.to_string(),
            });
        }
    }
    rejected.extend(unanswered);
    if executed.is_empty() {
        AgentDecision::RejectedTools {
            rejected_tools: rejected,
        }
    } else {
        AgentDecision::Dispatched {
            tools: executed,
            rejected_tools: rejected,
        }
    }
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
    /// Whole model call: request send → complete response / end of stream
    /// (`ChatMetadata::elapsed_ms`). Artifacts written before 2026-09-25
    /// held the streaming time-to-headers here instead.
    pub elapsed_ms: u64,
    /// Streaming only: send → response headers (stream established), a part
    /// of `elapsed_ms`. Not a time-to-first-token. Absent for non-streaming
    /// calls and in older artifacts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_to_headers_ms: Option<u64>,
    /// Shadow-mode evidence ledger state at the end of this turn.
    ///
    /// Written so recorded sessions can be inspected empirically: whether the
    /// citations name the right files, and what the debt curve actually looks
    /// like, before anything is tuned or displayed. Omitted from older
    /// artifacts, hence `default`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<EvidenceSnapshot>,
    /// Log probability information for tokens, when requested and returned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<serde_json::Value>,
}

/// What the ledger held at the end of a turn. Deliberately a summary plus
/// citations rather than the whole ledger: the artifact is for reading.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceSnapshot {
    /// Obligations still owed.
    pub outstanding: usize,
    /// Lines changed that nobody has read.
    pub unreviewed_lines: usize,
    /// Lines changed that no executed test covered.
    pub untested_lines: usize,
    /// Outstanding obligations whose size could not be determined. Non-zero
    /// means the line totals above are a floor, not a total.
    #[serde(default)]
    pub unknown_size_obligations: usize,
    /// Mutations that could not be attributed to a path, with their reasons.
    /// Persisted rather than debug-logged: this is the record that says whether
    /// the classifier's schema assumptions match real traffic.
    #[serde(default)]
    pub unattributed: Vec<crate::phi::observer::UnattributedRecord>,
    /// Commands observed this task, with outcomes and uncertainty.
    #[serde(default)]
    pub observations: Vec<crate::phi::observer::ObservationRecord>,
    /// Observations that may have changed files the ledger did not record. Any
    /// non-zero value means the totals above are a floor, not a total.
    #[serde(default)]
    pub possible_unrecorded_mutations: usize,
    /// One line per outstanding obligation, naming file and turn.
    pub citations: Vec<String>,
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
/// Uses THE config secret predicate (`crate::config::model::redact_config_secrets`):
/// every value under a secret-named key (`api_key`, `Authorization`,
/// `*_token`, `passwd`, … case- and `-`/`_`-insensitive) and every value of
/// an `env` / `headers` map becomes `"<redacted>"`, so artifacts, debug
/// request logs and config views agree on what a secret is.
pub fn sanitize_request_body(body: &mut serde_json::Value) {
    crate::config::model::redact_config_secrets(body);
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

/// Path of the artifact file for `step` under `dir`.
pub fn artifact_path(dir: &Path, step: usize) -> PathBuf {
    dir.join(format!("turn_{:04}.json", step))
}

/// The smallest step `>= from` whose artifact file does not exist yet in
/// `workdir`'s artifact directory. Artifact history is append-only: a new
/// turn never takes a slot an earlier process (or resumed segment) wrote.
pub fn next_free_step(workdir: &Path, from: usize) -> usize {
    let dir = artifact_dir(workdir);
    let mut step = from.max(1);
    while artifact_path(&dir, step).exists() {
        step += 1;
    }
    step
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
    let path = artifact_path(&dir, artifact.step);
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
