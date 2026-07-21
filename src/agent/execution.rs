use anyhow::{bail, Result};
use tracing::{debug, info};

use super::*;
use crate::errors::AgentError;
use crate::hooks::HookContext;

// Re-export items used by this module from sibling modules
pub(super) use super::recovery::ActionPrompt;

// Re-export type from tool_collect so existing call sites keep working
pub(super) use super::tool_collect::CollectedToolCall;

/// Default deadline (in milliseconds) to wait for the ESC listener to
/// acknowledge a pause request before giving up.  Kept as a `pub` constant
/// so callers or config layers can reference/override it if needed.
pub(super) const ESC_PAUSE_DEADLINE_MS: u64 = 250;

/// Read a line from stdin, temporarily pausing the ESC listener so it yields
/// raw mode and stops competing for stdin events.  This prevents the deadlock
/// where `io::stdin().read_line()` blocks forever because crossterm raw mode
/// is active on another thread.
pub(super) async fn read_line_pausing_esc(
    esc_paused: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    esc_pause_ack: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::io::Result<String> {
    read_line_pausing_esc_with_deadline(
        esc_paused,
        esc_pause_ack,
        tokio::time::Duration::from_millis(ESC_PAUSE_DEADLINE_MS),
    )
    .await
}

/// Same as [`read_line_pausing_esc`] but with an explicit ESC-ack deadline,
/// allowing callers to tune the wait when 250 ms is too short or too long.
pub(super) async fn read_line_pausing_esc_with_deadline(
    esc_paused: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    esc_pause_ack: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    esc_deadline: tokio::time::Duration,
) -> std::io::Result<String> {
    use std::sync::atomic::Ordering;
    use tokio::io::AsyncBufReadExt;

    // Signal the ESC listener to pause and release raw mode
    esc_paused.store(true, Ordering::Release);
    let deadline = tokio::time::Instant::now() + esc_deadline;
    while !esc_pause_ack.load(Ordering::Acquire) && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
    }

    let mut response = String::new();
    let stdin = tokio::io::stdin();
    let mut reader = tokio::io::BufReader::new(stdin);
    let result = reader.read_line(&mut response).await;

    // Unpause — the listener will re-enter raw mode on its own
    esc_paused.store(false, Ordering::Release);
    esc_pause_ack.store(false, Ordering::Release);

    result.map(|_| response)
}

/// Count source files in the current workdir (up to `cap`, then early-return).
///
/// Used by the ESCALATED progress guard to decide whether the workdir is a
/// real existing codebase (in which case the Rust scaffold injection is the
/// wrong move) or an empty SAB-style scratch project (where scaffold is fine).
///
/// We only need to know whether *any* substantial source exists, so we cap
/// the count and skip well-known build / dependency / cache directories. The
/// scaffold target `src/lib.rs` itself is excluded so that an empty SAB
/// template still reports zero.
pub(super) async fn count_source_files_in_workdir(cap: usize) -> usize {
    use std::path::Path;

    const SOURCE_EXTS: &[&str] = &[
        "rs", "py", "go", "js", "ts", "jsx", "tsx", "java", "rb", "c", "cpp", "cc", "cxx", "h",
        "hpp", "kt", "swift", "scala", "ml", "fs", "cs", "php", "lua", "ex", "exs", "erl", "clj",
        "dart", "zig",
    ];
    const EXCLUDED_DIRS: &[&str] = &[
        "target",
        "node_modules",
        "vendor",
        ".git",
        "__pycache__",
        "dist",
        "build",
        ".venv",
        "venv",
        "env",
        "out",
        ".next",
        ".cache",
        "site-packages",
    ];

    let workdir = match std::env::current_dir() {
        Ok(d) => d,
        Err(_) => return 0,
    };

    tokio::task::spawn_blocking(move || {
        let mut count = 0usize;
        let walker = walkdir::WalkDir::new(&workdir)
            .max_depth(4)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                if e.depth() == 0 {
                    return true;
                }
                !EXCLUDED_DIRS.contains(&name.as_ref())
            });

        for entry in walker.flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();

            // Skip the scaffold target itself.
            if path.file_name() == Some(std::ffi::OsStr::new("lib.rs"))
                && path.parent().and_then(Path::file_name) == Some(std::ffi::OsStr::new("src"))
                && path.parent().and_then(Path::parent) == Some(workdir.as_path())
            {
                continue;
            }

            let ext = match path.extension().and_then(|e| e.to_str()) {
                Some(e) => e.to_ascii_lowercase(),
                None => continue,
            };
            if !SOURCE_EXTS.contains(&ext.as_str()) {
                continue;
            }
            count += 1;
            if count >= cap {
                return count;
            }
        }
        count
    })
    .await
    .unwrap_or(0)
}

/// Try to extract a `base64_png` field from a JSON tool result string.
pub(super) fn try_extract_base64_png(result: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(result)
        .ok()?
        .get("base64_png")?
        .as_str()
        .map(String::from)
}

/// Build a text summary of a JSON tool result by removing the large `base64_png`
/// blob and adding an `"image_attached": true` marker.
pub(super) fn build_image_result_summary(result: &str) -> String {
    if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(result) {
        if let Some(obj) = v.as_object_mut() {
            obj.remove("base64_png");
            obj.insert("image_attached".to_string(), serde_json::Value::Bool(true));
        }
        serde_json::to_string(&v).unwrap_or_else(|_| result.to_string())
    } else {
        result.to_string()
    }
}

impl Agent {
    fn note_mutation_no_tool_stall(&mut self, context: &str) -> Result<()> {
        if !(self.current_task_requires_mutation() && self.mutating_tool_call_count() == 0) {
            return Ok(());
        }

        const MAX_MUTATION_NO_TOOL_STALLS: usize = 6;
        self.consecutive_no_action_prompts += 1;
        self.total_no_action_prompts += 1;
        if self.consecutive_no_action_prompts >= MAX_MUTATION_NO_TOOL_STALLS {
            bail!(
                "NONTERM_PROSE_NO_TOOL: mutation-required task produced {} consecutive no-tool turns with 0 mutating tools ({})",
                self.consecutive_no_action_prompts,
                context
            );
        }
        Ok(())
    }

    /// Lifetime abort for a mutation-required task that keeps producing tool-less
    /// turns with ZERO mutating tool calls — the model narrates "done" (or gets
    /// auto-recovered by the fallback) without ever editing. Both the
    /// ForceFallback path and the completion-gate-rejection path funnel through
    /// here so there is ONE owner: one counter (`mutation_gate_rejections`) and
    /// one threshold. Previously each path incremented the same counter but
    /// checked it against a different limit (8 vs 4), so the higher one was
    /// effectively dead and the abort point was hard to reason about. The counter
    /// is immune to the per-turn "fallback succeeded" reset — it clears only on a
    /// real mutating call (`note_mutating_tool_call`) — and aborts ~10× cheaper
    /// than letting the loop burn all iterations.
    fn note_mutation_zero_edit_stall(&mut self, diagnosis: &str) -> Result<()> {
        if !(self.current_task_requires_mutation() && self.mutating_tool_call_count() == 0) {
            return Ok(());
        }
        const MAX_MUTATION_ZERO_EDIT_STALLS: usize = 4;
        self.mutation_gate_rejections += 1;
        if self.mutation_gate_rejections >= MAX_MUTATION_ZERO_EDIT_STALLS {
            bail!(
                "FAKE_COMPLETE_LOOP: {} no-tool turns on a mutation-required task with 0 mutating \
                 tool calls ({}) — the model is not converging to an edit; aborting early instead \
                 of burning iterations",
                self.mutation_gate_rejections,
                diagnosis
            );
        }
        Ok(())
    }

    /// P2.1 FILES: checklist guard. Returns true when a file-write tool call
    /// should be blocked because no files checklist has been seen and no prior
    /// write has succeeded. `has_file_write_intent` must only count calls that
    /// write file content (see `tool_call_is_file_write_intent`) — mutating
    /// shell commands like mkdir/npm install must not trip this guard. When
    /// force-mutation recovery is pending, the guard is bypassed once and the
    /// pending flag is consumed.
    fn check_files_guard(&mut self, has_file_write_intent: bool) -> bool {
        let blocked = has_file_write_intent
            && !self.has_written_any_file
            && !self.files_checklist_seen
            && !self.force_mutation_pending;
        if !blocked && has_file_write_intent && self.force_mutation_pending {
            self.force_mutation_pending = false;
        }
        blocked
    }

    /// True when the tool batch contains at least one write and EVERY write
    /// targets a file the agent has already read. Such an edit is not "blind"
    /// (the read established which file changes), so the FILES: checklist guard
    /// should not block it. Returns false if there are no writes, or any write
    /// targets a file that was never read (a genuinely blind edit / new file).
    fn writes_target_only_read_files(&self, tool_calls: &[CollectedToolCall]) -> bool {
        let mut saw_write = false;
        for (name, args_str, _) in tool_calls {
            if !tool_call_is_file_write_intent(name, args_str) {
                continue;
            }
            saw_write = true;
            let target_read = serde_json::from_str::<serde_json::Value>(args_str)
                .ok()
                .and_then(|v| v.get("path").and_then(|p| p.as_str()).map(String::from))
                .map(|p| self.file_tracker.read_state.contains_key(&p))
                .unwrap_or(false);
            if !target_read {
                return false;
            }
        }
        saw_write
    }

    /// True if `line` looks like a `FILES:` checklist header.
    ///
    /// Tolerant of leading markdown decoration and case, because capable hosted
    /// models (e.g. GLM-4.x/5.x) format the header as `**FILES:**`, `- Files:`,
    /// or `#### FILES:` rather than a bare `FILES:`. Without this tolerance the
    /// P2.1 edit guard never sees the checklist, blocks every edit, and the model
    /// loops emitting the (unrecognized) header as prose — surfacing as
    /// `NONTERM_PROSE_NO_TOOL`.
    fn is_files_checklist_line(line: &str) -> bool {
        let cleaned = line.trim_start().trim_start_matches(|c: char| {
            matches!(c, '*' | '#' | '`' | '-' | '>' | '_' | '•' | ' ' | '\t')
        });
        cleaned.to_ascii_lowercase().starts_with("files:")
    }

    /// Execute a step with tool call logging for checkpoints
    /// If `use_last_message` is true, process tool calls from the last assistant message
    /// instead of making a new API call (used after planning phase)
    pub(super) async fn execute_step_with_logging(
        &mut self,
        _task_description: &str,
    ) -> Result<bool> {
        self.execute_step_internal(false).await
    }

    /// Execute tool calls from the last assistant message (after planning)
    pub(super) async fn execute_pending_tool_calls(
        &mut self,
        _task_description: &str,
    ) -> Result<bool> {
        self.execute_step_internal(true).await
    }

    /// Write a per-turn debug artifact for the current step.
    ///
    /// Called from within `execute_step_internal` once we know what the
    /// agent decided to do with the model's response.  Honours
    /// `agent.disable_turn_artifacts`; failures are logged, never returned.
    pub(super) async fn write_turn_artifact(
        &self,
        step: usize,
        chat_metadata: Option<&crate::api::types::ChatMetadata>,
        parsed_tool_calls: &[crate::api::types::ToolCall],
        decision: super::turn_artifacts::AgentDecision,
        response_text: &str,
        reasoning_content: Option<&str>,
    ) {
        // Surface a live progress event for turn decisions that would otherwise
        // be invisible (a refused / no-op / nudged / aborted turn currently
        // renders as step_started -> step_completed with nothing between). This
        // runs BEFORE the artifact-disabled guard so live visibility does not
        // depend on turn artifacts being enabled. Tool-executing and completed
        // turns are already visible via tool events / task_completed, so they
        // are intentionally not re-emitted here.
        {
            use super::turn_artifacts::AgentDecision as AD;
            let live: Option<(&str, String)> = match &decision {
                AD::NoToolCall => Some(("no_tool_call", String::new())),
                AD::NudgeInjected { reason } => {
                    Some(("nudge_injected", reason.chars().take(120).collect()))
                }
                AD::Aborted { reason } => Some(("aborted", reason.chars().take(120).collect())),
                AD::Refused { reason } => Some(("refused", reason.chars().take(120).collect())),
                AD::ExecutedTools { .. } | AD::Completed { .. } => None,
            };
            if let Some((decision_name, detail)) = live {
                self.emit_progress(super::progress::ProgressEvent::TurnDecision {
                    decision: decision_name.to_string(),
                    detail,
                });
            }
        }

        if self.config.agent.disable_turn_artifacts {
            return;
        }
        // Skip when there's no fresh API call for this step — replays of
        // prior assistant messages (use_last_message=true) don't have a
        // request body to capture, and cache hits leave request_body empty.
        let Some(meta) = chat_metadata else {
            return;
        };
        let body_is_empty = meta.request_body.is_null()
            || meta
                .request_body
                .as_object()
                .map(|o| o.is_empty())
                .unwrap_or(true);
        if body_is_empty {
            return;
        }
        let mut request_body = meta.request_body.clone();
        super::turn_artifacts::sanitize_request_body(&mut request_body);

        // Reconstruct a minimal response_body from what we have. Streaming
        // never gives us back the original JSON — we build a faithful shape
        // that includes the assembled assistant message + reasoning_content
        // + parsed tool calls so the artifact mirrors the on-the-wire format.
        let response_body = serde_json::json!({
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": response_text,
                    "reasoning_content": reasoning_content,
                    "tool_calls": parsed_tool_calls,
                },
                "finish_reason": meta.finish_reason,
            }],
            "usage": {
                "prompt_tokens": meta.prompt_tokens,
                "completion_tokens": meta.completion_tokens,
            },
        });

        let workdir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let artifact = super::turn_artifacts::TurnArtifact {
            step,
            timestamp: chrono::Utc::now(),
            request_body,
            response_body,
            finish_reason: meta.finish_reason.clone(),
            completion_tokens: meta.completion_tokens,
            prompt_tokens: meta.prompt_tokens,
            reasoning_content: reasoning_content.map(|s| s.to_string()),
            parsed_tool_calls: parsed_tool_calls.to_vec(),
            agent_decision: decision,
            elapsed_ms: meta.elapsed_ms,
        };
        super::turn_artifacts::write_artifact(&workdir, &artifact).await;
    }

    /// Internal execution logic
    /// If `use_last_message` is true, process tool calls from the last assistant message
    ///
    /// Wraps [`Self::execute_step_internal_inner`] so a `StepCompleted` progress
    /// event always fires after the step body returns — even on early returns
    /// from guards or error paths.  The inner function's many `return Ok(...)`
    /// branches still work; we just emit one trailing event when it unwinds.
    async fn execute_step_internal(&mut self, use_last_message: bool) -> Result<bool> {
        let step_at_entry = self.loop_control.current_step().saturating_add(1);
        let result = self.execute_step_internal_inner(use_last_message).await;
        self.emit_progress(super::progress::ProgressEvent::StepCompleted {
            step: step_at_entry,
            mutating_tools_so_far: self.mutating_tool_call_count(),
        });
        result
    }

    /// Update `consecutive_read_only_steps` and `seen_read_targets` based on
    /// the tool calls in the current step.
    ///
    /// When the step contains a write/edit/delete tool, both the counter and
    /// the seen-set are cleared (the agent made progress).
    ///
    /// When the step is read-only, the **investigation-progress reset** (bug
    /// #13) kicks in: if any read-only tool targets a *novel* file path / grep
    /// query (not yet in `seen_read_targets`), the counter is **decremented**
    /// because the agent is exploring new ground.  Only when every read target
    /// is redundant (already seen this streak) does the counter increment,
    /// ensuring true infinite re-read loops still climb to the threshold.
    pub(super) fn update_read_only_step_tracking(
        &mut self,
        tool_calls: &[super::execution::CollectedToolCall],
        has_write_tool: bool,
    ) {
        if has_write_tool {
            self.consecutive_read_only_steps = 0;
            self.seen_read_targets.clear();
            return;
        }

        let has_novel_target = tool_calls.iter().any(|(name, args_str, _)| {
            if !super::tool_dispatch::tool_call_is_observational(name, args_str) {
                return false;
            }
            match super::tool_dispatch::read_tool_target(name, args_str) {
                Some(target) => !self.seen_read_targets.contains(&target),
                None => false,
            }
        });

        for (name, args_str, _) in tool_calls {
            if super::tool_dispatch::tool_call_is_observational(name, args_str) {
                if let Some(target) = super::tool_dispatch::read_tool_target(name, args_str) {
                    self.seen_read_targets.insert(target);
                }
            }
        }

        if has_novel_target {
            self.consecutive_read_only_steps = self.consecutive_read_only_steps.saturating_sub(1);
        } else {
            self.consecutive_read_only_steps += 1;
        }
    }

    /// Pick the verification command to auto-run when the model keeps
    /// answering without re-verifying (the StaleVerification churn breaker).
    /// Rust projects keep the original `cargo_check` rescue; for any other
    /// project the language-agnostic signal is the project's own test/check
    /// command — the most recent verification-framed shell command the model
    /// itself already ran this run is re-executed so the gate gains fresh
    /// evidence and the run converges (P0-2). Returns `None` when there is
    /// no honest command to run.
    /// The tuple is `(tool_name, tool_args_json, display_command)`.
    fn stale_verification_rescue_call(&self) -> Option<(String, String, String)> {
        let is_rust = super::current_project_root().join("Cargo.toml").exists();
        if is_rust && self.tools.get("cargo_check").is_some() {
            return Some((
                "cargo_check".to_string(),
                "{}".to_string(),
                "cargo_check".to_string(),
            ));
        }
        let command = self
            .current_checkpoint
            .as_ref()?
            .tool_calls
            .iter()
            .rev()
            .filter(|tc| matches!(tc.tool_name.as_str(), "shell_exec" | "pty_shell"))
            .filter_map(|tc| {
                serde_json::from_str::<serde_json::Value>(&tc.arguments)
                    .ok()?
                    .get("command")?
                    .as_str()
                    .map(str::to_string)
            })
            .find(|cmd| super::tool_dispatch::shell_command_is_verification(cmd))?;
        self.tools.get("shell_exec")?;
        Some((
            "shell_exec".to_string(),
            serde_json::json!({"command": command}).to_string(),
            command,
        ))
    }

    async fn execute_step_internal_inner(&mut self, use_last_message: bool) -> Result<bool> {
        // Emit a structured progress event at the top of every loop iteration.
        // Step is 1-based; tool count is the registry's current size.
        self.emit_progress(super::progress::ProgressEvent::StepStarted {
            step: self.loop_control.current_step().saturating_add(1),
            model: self.config.model.clone(),
            tools_available: self.tools.definitions().len(),
        });

        let response = self.get_assistant_step_response(use_last_message).await?;
        let content = response.content;
        let reasoning_chars = response.reasoning_chars;
        // Snapshot the per-call metadata before we move out of `response` —
        // the per-turn debug capture below uses it to write the artifact.
        let chat_metadata = response.metadata.clone();

        let tool_calls = self.collect_tool_calls(
            &content,
            response.reasoning_content.as_deref(),
            response.native_tool_calls.as_ref(),
        );

        // Detect a FILES: checklist in the assistant response. Small models often
        // jump straight to editing; requiring them to name the files first
        // reduces catastrophic edits on the wrong files.
        if super::recovery::strip_think_blocks(&content)
            .lines()
            .any(Self::is_files_checklist_line)
        {
            self.files_checklist_seen = true;
        }

        // Descriptive step line, printed now that the model's action for this step
        // is known (a plain "Step N Executing..." before the call is uninformative).
        // Shows the tool(s) being run with a hint of the first arg, or that the
        // turn is a final answer / narration with no tool call.
        {
            let step_no = self.loop_control.current_step().saturating_add(1);
            let desc = if let Some((name, args, _)) = tool_calls.first() {
                let hint = serde_json::from_str::<serde_json::Value>(args)
                    .ok()
                    .and_then(|v| {
                        ["path", "command", "query", "pattern", "file"]
                            .iter()
                            .find_map(|k| v.get(*k).and_then(|x| x.as_str()).map(String::from))
                    })
                    .map(|s| s.chars().take(56).collect::<String>());
                let extra = if tool_calls.len() > 1 {
                    format!(" (+{} more)", tool_calls.len() - 1)
                } else {
                    String::new()
                };
                match hint {
                    Some(h) => format!("→ {} {}{}", name, h, extra),
                    None => format!("→ {}{}", name, extra),
                }
            } else if super::recovery::strip_think_blocks(&content).trim().len() >= 40 {
                "→ final answer".to_string()
            } else {
                "→ thinking (no tool call)".to_string()
            };
            output::step_start(step_no, &desc);
        }

        // Convert XML-extracted CollectedToolCall back to the structured
        // ToolCall shape that goes into the artifact.  Use a deterministic
        // synthetic id when the XML branch produced none.
        let parsed_tool_calls_for_artifact: Vec<crate::api::types::ToolCall> =
            response.native_tool_calls.clone().unwrap_or_else(|| {
                tool_calls
                    .iter()
                    .enumerate()
                    .map(|(i, (name, args, id))| crate::api::types::ToolCall {
                        id: id.clone().unwrap_or_else(|| format!("xml_{}", i)),
                        call_type: "function".to_string(),
                        function: crate::api::types::ToolFunction {
                            name: name.clone(),
                            arguments: args.clone(),
                        },
                    })
                    .collect()
            });
        // Tool names in dispatch order — recorded for the AgentDecision.
        let tool_names_for_artifact: Vec<String> =
            tool_calls.iter().map(|(n, _, _)| n.clone()).collect();
        // Reserve the next sequence number now so the file naming matches
        // the order calls happened, even when nested helpers also write.
        let artifact_step = {
            self.turn_artifact_seq += 1;
            self.turn_artifact_seq
        };

        debug!("Total tool calls to execute: {}", tool_calls.len());

        // Per-turn debug capture: write `<workdir>/.selfware/turns/turn_NNNN.json`
        // immediately after parsing.  Decision is the obvious split between
        // "executed N tools" and "model returned no tool call" — refusal /
        // nudge / completion classifications are visible from the agent
        // decision branches below, but writing the artifact early ensures we
        // never lose it when later code panics or short-circuits unexpectedly.
        let initial_decision = if parsed_tool_calls_for_artifact.is_empty() && tool_calls.is_empty()
        {
            super::turn_artifacts::AgentDecision::NoToolCall
        } else {
            super::turn_artifacts::AgentDecision::ExecutedTools {
                tools: tool_names_for_artifact.clone(),
            }
        };
        self.write_turn_artifact(
            artifact_step,
            chat_metadata.as_ref(),
            &parsed_tool_calls_for_artifact,
            initial_decision,
            &content,
            response.reasoning_content.as_deref(),
        )
        .await;

        // Check if the response contains code alongside tool calls.
        // Models often output file_read tool calls AND code text in the same
        // response. The tool calls get executed, but the code gets ignored.
        // If we detect code in the residual text, auto-write it.
        //
        // SAFETY GATE: when verification is required before completion, defer
        // the auto-write until the completion gate has passed. Writing
        // unverified code to disk before the gate runs means edits can persist
        // even when the task should be rejected. If the gate has not yet
        // passed, we nudge the model to use file_write/file_edit explicitly
        // instead, so the normal tool path (with verification) applies.
        if !tool_calls.is_empty() && contains_unwritten_code(&content) {
            if let Some((path, code)) = extract_code_and_path(&content).await {
                let gate_ok = if self.config.agent.require_verification_before_completion {
                    self.check_completion_gate().await.is_none()
                } else {
                    true
                };
                if gate_ok {
                    info!(
                        "Response contains tool calls AND code text — auto-writing {} lines to {}",
                        code.lines().count(),
                        path
                    );
                    let write_call: Vec<super::execution::CollectedToolCall> = vec![(
                        "file_write".to_string(),
                        serde_json::json!({"path": path, "content": code}).to_string(),
                        None,
                    )];
                    // Prepend the file_write to the tool calls list would be ideal,
                    // but we'll execute it after the original tool calls
                    self.execute_tool_batch(write_call).await?;
                    self.consecutive_read_only_steps = 0;
                    self.seen_read_targets.clear();
                    self.has_written_any_file = true;
                    self.terminal_guard_hits = 0;
                    self.messages.push(crate::api::types::Message::user(
                        "<selfware_system_directive>\n\
                         Code from your response was automatically written to a file. \
                         Now verify with cargo check or cargo test.\n\
                         </selfware_system_directive>"
                            .to_string(),
                    ));
                } else {
                    info!(
                        "Response contains code text for {} but completion gate not yet passed — deferring auto-write, nudging to use tools",
                        path
                    );
                    self.messages.push(crate::api::types::Message::user(
                        "<selfware_system_directive>\n\
                         Your response contains code that should be written to a file. \\\n\
                         Use the file_write or file_edit tool to write it, then verify with \\\n\
                         cargo check or cargo test. Do NOT output code as plain text.\n\
                         </selfware_system_directive>"
                            .to_string(),
                    ));
                }
            }
        }

        // Detect malformed tool calls and inject correction before treating as completion
        if self.detect_and_correct_malformed_tools(&content, &tool_calls) {
            self.note_mutation_no_tool_stall("malformed tool XML")?;
            return Ok(false);
        }

        // Gate-satisfied completion: if the completion gate is ALREADY satisfied
        // (for a mutation task: edits made + verification passed) and the model
        // returns a substantial, non-confused tool-less answer, accept it now.
        // The completion gate is the authority on "done"; without this, a valid
        // summary answer that happens to contain an intent phrase ("let me ...",
        // "I'll ...") gets re-classified by maybe_prompt_for_action as "describing
        // intent" → ForceFallback, and since edits are done no fake-complete guard
        // trips, so the loop spins empty steps to MAX_ITERATIONS even though the
        // code compiles and tests pass (observed on multi-file tasks).
        if tool_calls.is_empty() {
            let clean = super::recovery::strip_think_blocks(&content)
                .trim()
                .to_string();
            if clean.len() >= 40
                && !super::verification::is_confused_response(&content)
                && !super::verification::is_incomplete_action_response(&content)
                && self.check_completion_gate().await.is_none()
            {
                info!("Completion gate satisfied — accepting substantial final answer");
                output::final_answer(&clean);
                self.last_assistant_response = clean;
                return Ok(true);
            }
        }

        // Read-only completion gate. For a non-mutating task (code review /
        // analysis / question) that returns no tool call, accept a substantial
        // final answer immediately; and if the model instead keeps narrating
        // ("let me check ...") without answering, force-finalize after a few such
        // turns rather than spinning to MAX_ITERATIONS (observed: read-only
        // reviews looped 100 steps and were mislabeled FAKE_COMPLETE). This ONLY
        // adds an acceptance/finalize path — when neither condition holds it falls
        // through to the existing flow, so normal short/empty completions and all
        // mutation tasks are unaffected (the FAKE_COMPLETE edit gate stays intact).
        if tool_calls.is_empty() && !self.current_task_requires_mutation() {
            let clean = super::recovery::strip_think_blocks(&content)
                .trim()
                .to_string();
            let looks_final = clean.len() >= 40
                && !super::verification::is_incomplete_action_response(&content)
                && !super::verification::is_confused_response(&content);
            if looks_final && self.check_completion_gate().await.is_none() {
                info!("Read-only task: accepting substantial final answer");
                output::final_answer(&clean);
                self.last_assistant_response = clean;
                return Ok(true);
            }
            // Track the LATEST substantial answer, not the longest ever seen — a
            // later corrected answer must not be discarded in favor of an earlier,
            // longer, wrong one (GATE-FORCE-FINALIZE).
            if clean.len() >= 40 {
                self.last_assistant_response = clean.clone();
            }
            self.readonly_no_tool_streak += 1;
            if self.readonly_no_tool_streak >= 6 && self.last_assistant_response.len() >= 40 {
                // Prefer to force-finalize only when the completion gate is
                // satisfied — otherwise we would emit a capability-disclaimer or a
                // response missing a required tool as the "final answer" (found and
                // fixed by GLM-5.2's own cycle-2 self-improvement pass; folded in).
                // But keep a higher hard cap so a gate that keeps rejecting can't
                // spin the read-only task to MAX_ITERATIONS — termination is still
                // guaranteed as a last resort.
                let gate_ok = self.check_completion_gate().await.is_none();
                if gate_ok || self.readonly_no_tool_streak >= 12 {
                    info!(
                        "Read-only task: force-finalizing after {} no-tool turns (gate_ok={})",
                        self.readonly_no_tool_streak, gate_ok
                    );
                    let best = self.last_assistant_response.clone();
                    output::final_answer(&best);
                    return Ok(true);
                }
                // Gate rejected and under the hard cap — fall through so the gate's
                // rejection nudge is injected and the model can correct.
            }
            // else: fall through to the existing completion/fallback handling.
        }

        match self.maybe_prompt_for_action(
            &content,
            tool_calls.is_empty(),
            use_last_message,
            reasoning_chars,
        ) {
            Ok(ActionPrompt::Corrected) => return Ok(false),
            Ok(ActionPrompt::NotNeeded) => {}
            Ok(ActionPrompt::ForceFallback) => {
                let (tool_name, tool_args) = self.pick_smart_fallback(&content);

                // Lifetime stall accounting for mutation tasks. The fallback below
                // "succeeds" and resets the consecutive counter every turn, so a model
                // that keeps emitting tool-less final answers on a mutation task loops
                // to MAX_ITERATIONS (observed at $4–$22/run). This counter is immune
                // to that reset — it clears only on a real mutating call — and aborts
                // early with the same FAKE_COMPLETE diagnosis, ~10× cheaper.
                self.note_mutation_zero_edit_stall("no-tool turns auto-recovered by fallback")?;

                // If the only fallback is directory_tree (nothing new to load),
                // the model is stuck and should just answer with what it has.
                if tool_name == super::FALLBACK_TOOL_NAME {
                    if self.current_task_requires_mutation() && self.mutating_tool_call_count() == 0
                    {
                        bail!(
                            "NONTERM_PROSE_NO_TOOL: mutation-required task produced repeated prose-only turns with 0 mutating tools"
                        );
                    }
                    info!("No useful fallback available — prompting model to finalize");
                    self.messages.push(crate::api::types::Message::user(
                        "<selfware_system_directive>\n\
                         You have been unable to call tools. You already have the codebase \
                         overview and any files you previously read in context.\n\
                         Provide your answer now based on what you already know. \
                         Do not describe what you would do — just give the answer.\n\
                         </selfware_system_directive>"
                            .to_string(),
                    ));
                    return Ok(false);
                }

                info!("Smart fallback: {} {}", tool_name, tool_args);
                output::smart_fallback_action(&tool_name, &tool_args);
                let fallback: Vec<CollectedToolCall> = vec![(tool_name.clone(), tool_args, None)];
                self.execute_tool_batch(fallback).await?;

                // Successful fallback = progress. Reset consecutive counter
                // so it doesn't accumulate toward abort. The total counter
                // still tracks lifetime attempts for the hard ceiling.
                self.consecutive_no_action_prompts = 0;

                self.messages.push(crate::api::types::Message::user(format!(
                    "<selfware_system_directive>\n\
                     A `{}` was executed automatically because you did not call any tool.\n\
                     Use the result above to choose your next action. Call a tool now.\n\
                     </selfware_system_directive>",
                    tool_name
                )));
                return Ok(false);
            }
            Err(error_msg) => {
                return Err(AgentError::TaskFailed { message: error_msg }.into());
            }
        }

        if tool_calls.is_empty() {
            if super::verification::is_incomplete_action_response(&content) {
                info!("Rejected incomplete planning response before completion");
                self.last_assistant_response = content.clone();
                self.messages.push(crate::api::types::Message::user(
                    "Your response describes work you still need to do instead of a completed result. \
                     Do NOT stop to narrate your next step. Call the needed tool now and continue."
                        .to_string(),
                ));
                return Ok(false);
            }

            // Detect repeated identical responses — the model is stuck producing
            // the same completion that keeps getting rejected by the gate.
            if content == self.last_assistant_response && !content.is_empty() {
                self.consecutive_no_action_prompts += 1;
                if self.consecutive_no_action_prompts >= 5 {
                    // Model is repeating itself many times. Only accept if
                    // the completion gate passes — otherwise nudge it.
                    if self.check_completion_gate().await.is_none() {
                        info!(
                            "Accepting repeated completion after {} identical responses (gate passed)",
                            self.consecutive_no_action_prompts
                        );
                        let clean = super::recovery::strip_think_blocks(&content)
                            .trim()
                            .to_string();
                        output::final_answer(&clean);
                        self.last_assistant_response = clean;
                        return Ok(true);
                    }
                    // Gate rejected — tell the model to keep working.
                    // Do NOT reset the counter to zero; keep it at the threshold so
                    // the next identical response immediately re-checks the gate
                    // instead of looping through five more identical turns.
                    info!(
                        "Completion gate rejected after {} identical responses — nudging",
                        self.consecutive_no_action_prompts
                    );
                    self.consecutive_no_action_prompts = 5;
                    self.messages.push(crate::api::types::Message::user(
                        "<selfware_system_directive>\n\
                         You keep repeating the same response, but the task is NOT complete. \
                         You still need to verify your work. Call a tool now.\n\
                         </selfware_system_directive>"
                            .to_string(),
                    ));
                    return Ok(false);
                }
            } else {
                // Different response — store it for comparison.
                self.last_assistant_response = content.clone();
            }

            // Reject confused meta-reasoning before treating as completion
            if super::verification::is_confused_response(&content) {
                info!("Rejected confused meta-reasoning response");
                self.messages.push(crate::api::types::Message::user(
                    "<selfware_system_directive>\n\
                     Your response contained framework self-reference instead of a task result. \
                     Focus on the original task.\n\
                     </selfware_system_directive>"
                        .to_string(),
                ));
                return Ok(false);
            }

            if super::verification::is_capability_disclaimer_response(&content) {
                let has_successful_tool_calls = self
                    .current_checkpoint
                    .as_ref()
                    .map(|cp| cp.tool_calls.iter().any(|tc| tc.success))
                    .unwrap_or(false);
                if has_successful_tool_calls {
                    info!("Rejected false capability disclaimer after successful tool use");
                    if self.pending_synthesis.is_none() {
                        let synthesis_task = self
                            .current_checkpoint
                            .as_ref()
                            .map(|cp| cp.task_description.clone())
                            .unwrap_or_else(|| self.learning_context().to_string());
                        self.pending_synthesis = Some(synthesis_task);
                    }
                    self.messages.push(crate::api::types::Message::user(
                        "<selfware_system_directive>\n\
                         You already executed tools successfully in this session. \
                         Use the tool results that are already in context and answer the task directly. \
                         Do NOT claim you cannot access tools, files, or the local filesystem. \
                         A synthesis pass will answer from the tool results if needed.\n\
                         </selfware_system_directive>"
                            .to_string(),
                    ));
                    return Ok(false);
                }
            }

            // Detect text responses that contain code — the model should
            // use file_write/file_edit tools, not output code as text.
            // Auto-writing assistant text was removed so that the completion
            // gate runs before any state mutation and cannot be bypassed by a
            // synthetic file_write that masks unverified code.
            if contains_unwritten_code(&content) {
                info!("Rejected text response containing code — nudging to use tools");
                self.note_mutation_no_tool_stall("code-like text without extractable path")?;
                self.messages.push(crate::api::types::Message::user(
                    "<selfware_system_directive>\n\
                     You wrote code in your text response instead of using tools. \
                     DO NOT output code as text. Use file_edit on the existing target file you already read. \
                     Do NOT create src/lib.rs unless the repository already is a Rust crate.\n\n\
                     <tool>\n<name>file_write</name>\n\
                     <arguments>{\"path\": \"PATH_YOU_ALREADY_READ\", \"content\": \"FULL UPDATED FILE CONTENT\"}</arguments>\n\
                     </tool>\n\
                     </selfware_system_directive>"
                        .to_string(),
                ));
                return Ok(false);
            }

            // Check completion gate before accepting task as done
            if let Some(gate_msg) = self.check_completion_gate().await {
                info!("Completion gate rejected: {}", gate_msg);
                // Early hard-stop for the fake-complete loop: on a mutation-required
                // task with zero mutating calls, the model alternating {final answer →
                // gate rejection → read-only tool} resets every consecutive counter and
                // previously burned all 100 iterations (observed: $4–$22 per run before
                // MAX_ITERATIONS). This lifetime counter is immune to those resets.
                self.note_mutation_zero_edit_stall("completion gate rejected the final answer")?;
                // Churn breaker: the edit is done but verification hasn't passed and
                // the model keeps answering instead of running the test
                // (StaleVerification). After a couple of these, run the verification
                // ourselves so the run converges instead of wasting turns/tokens.
                let is_stale_verification = tool_calls.is_empty()
                    && self.mutating_tool_call_count() > 0
                    && (gate_msg.contains("StaleVerification")
                        || gate_msg.contains("verification has not passed")
                        || gate_msg.contains("FailingTestsAccepted"));
                if is_stale_verification {
                    self.consecutive_stale_verification += 1;
                    if self.consecutive_stale_verification >= 2 {
                        if let Some((tool_name, tool_args, display_cmd)) =
                            self.stale_verification_rescue_call()
                        {
                            info!(
                                "Auto-running `{}` to break StaleVerification churn",
                                display_cmd
                            );
                            self.consecutive_stale_verification = 0;
                            self.tools.activate(&tool_name);
                            let batch: Vec<CollectedToolCall> = vec![(tool_name, tool_args, None)];
                            self.execute_tool_batch(batch).await?;
                            self.messages.push(crate::api::types::Message::user(format!(
                                "<selfware_system_directive>\n\
                                 `{display_cmd}` was run automatically. If it passed, give your final \
                                 answer now; if it reported errors, fix them and re-verify.\n\
                                 </selfware_system_directive>"
                            )));
                            return Ok(false);
                        }
                    }
                } else {
                    self.consecutive_stale_verification = 0;
                }
                // Refine the previously-written turn artifact: this turn ended
                // with a gate refusal, not a plain "no tool call".
                self.write_turn_artifact(
                    artifact_step,
                    chat_metadata.as_ref(),
                    &parsed_tool_calls_for_artifact,
                    super::turn_artifacts::AgentDecision::Refused {
                        reason: gate_msg.clone(),
                    },
                    &content,
                    response.reasoning_content.as_deref(),
                )
                .await;
                self.messages
                    .push(crate::api::types::Message::user(gate_msg));
                return Ok(false);
            }

            // An empty response is a provider hiccup / dropped stream, not a valid
            // completion — never accept it as the final answer (that would paint a
            // ✅ banner with no content). Nudge the model to actually answer.
            let clean_final = super::recovery::strip_think_blocks(&content)
                .trim()
                .to_string();
            if clean_final.is_empty() {
                info!("Rejected empty response as final answer — nudging for an actual answer");
                self.messages.push(crate::api::types::Message::user(
                    "<selfware_system_directive>\n\
                     Your last response was empty. Provide your actual final answer now \
                     (a concise summary of the completed work).\n\
                     </selfware_system_directive>"
                        .to_string(),
                ));
                return Ok(false);
            }

            // A response cut off by the token limit (finish_reason == "length") is
            // incomplete — don't accept the truncated text as the final answer; ask
            // the model to finish.
            if chat_metadata
                .as_ref()
                .and_then(|m| m.finish_reason.as_deref())
                == Some("length")
            {
                info!("Rejected length-truncated response as final answer — asking to continue");
                self.messages.push(crate::api::types::Message::user(
                    "<selfware_system_directive>\n\
                     Your previous response was cut off by the length limit before it \
                     finished. Continue and complete your answer concisely.\n\
                     </selfware_system_directive>"
                        .to_string(),
                ));
                return Ok(false);
            }

            // Fire Stop hooks before completing
            let stop_ctx = HookContext::stop();
            self.hook_registry.fire(&stop_ctx).await;

            // Strip think blocks from the final answer — the content accumulator
            // includes raw <think>...</think> tags from models like Qwen3.5 that
            // emit inline thinking. Without stripping, "Final answer:" shows the
            // think block content instead of the actual response.
            let clean_content = super::recovery::strip_think_blocks(&content)
                .trim()
                .to_string();
            output::final_answer(&clean_content);
            // Refine the artifact decision: this turn ended in a final answer.
            self.write_turn_artifact(
                artifact_step,
                chat_metadata.as_ref(),
                &parsed_tool_calls_for_artifact,
                super::turn_artifacts::AgentDecision::Completed {
                    text: clean_content.clone(),
                },
                &content,
                response.reasoning_content.as_deref(),
            )
            .await;
            self.last_assistant_response = clean_content;
            return Ok(true);
        }

        let context_target =
            (!self.current_task_context.is_empty()).then_some(self.current_task_context.as_str());
        let literal_target = self
            .current_checkpoint
            .as_ref()
            .map(|cp| cp.task_description.as_str())
            .or(context_target)
            .and_then(super::verification::exact_response_target);

        if let Some(target) = literal_target {
            info!("Rejected tool calls for exact-response task");
            self.messages.push(crate::api::types::Message::user(format!(
                "<selfware_system_directive>\n\
                 Do NOT use any tools for this task.\n\
                 Reply with exactly this text and nothing else:\n\
                 {}\n\
                 </selfware_system_directive>",
                target
            )));
            return Ok(false);
        }

        // Plan mode: show proposed tool calls without executing
        if self.plan_mode {
            let mut plan_summary =
                String::from("Plan mode — proposed tool calls (not executed):\n");
            for (i, (name, args_str, _)) in tool_calls.iter().enumerate() {
                let args_preview: String = args_str.chars().take(200).collect();
                plan_summary.push_str(&format!(
                    "\n{}. {} — {}{}\n",
                    i + 1,
                    name,
                    args_preview,
                    if args_str.len() > 200 { "..." } else { "" }
                ));
            }
            output::final_answer(&plan_summary);
            self.messages.push(crate::api::types::Message::user(
                "The above tool calls were proposed but NOT executed (plan mode is active). \
                 Review the plan and confirm, or adjust.",
            ));
            self.last_assistant_response = plan_summary;
            return Ok(false);
        }

        // Detect repetition loops before executing
        if let Some(loop_msg) = self.detect_repetition(&tool_calls) {
            if self.current_task_requires_mutation()
                && self.mutating_tool_call_count() > 0
                && is_observational_shell_batch(&tool_calls)
            {
                self.post_edit_observational_shell_count += 1;
                if self.post_edit_observational_shell_count > 2 {
                    bail!(
                        "VERIFICATION_LOOP_AFTER_EDIT: repeated observational shell commands after a successful mutation; stopping so the captured patch can be evaluated"
                    );
                }
                info!(
                    "Post-edit observational shell loop detected ({}); injecting correction",
                    self.post_edit_observational_shell_count
                );
            } else {
                info!("Repetition loop detected, injecting correction");
            }
            self.messages
                .push(crate::api::types::Message::user(loop_msg));
            return Ok(false);
        }

        self.reset_no_action_prompt_state();

        // Track whether this step contains any write/modify tools
        let has_write_tool = tool_calls.iter().any(|(name, args_str, _)| {
            super::tool_dispatch::tool_call_counts_as_state_change(name, args_str)
        });
        self.update_read_only_step_tracking(&tool_calls, has_write_tool);

        // The FILES: checklist guard exists to stop blind FILE EDITS. A shell
        // command can mutate plenty (mkdir, npm/pip installs, git ops, builds)
        // without writing file content — classifying those as "writes" made
        // the guard silently discard legitimate setup commands and burn turns
        // re-issuing them. Track file-write intent separately for the guard.
        let has_file_write_intent = tool_calls
            .iter()
            .any(|(name, args_str, _)| tool_call_is_file_write_intent(name, args_str));

        // A write to a file the agent has ALREADY READ is not a blind edit — the
        // read already established which file changes, so the FILES: checklist
        // ceremony is redundant. Treat such a write as satisfying the checklist.
        // Without this, the guard silently discarded a capable model's first
        // (correct) edit; the model then believed it had already edited and
        // stalled to FAKE_COMPLETE with 0 files changed (the "doable task" bug).
        if has_file_write_intent
            && !self.files_checklist_seen
            && self.writes_target_only_read_files(&tool_calls)
        {
            info!(
                "Allowing edit to already-read file(s) — FILES: checklist satisfied by prior read"
            );
            self.files_checklist_seen = true;
        }

        // P2.1: require a FILES: checklist before the first mutating edit.
        // Force-mutation recovery bypasses this once: after the progress guard
        // has explicitly demanded an immediate edit, do not block the edit for
        // lacking a FILES: line.
        if self.check_files_guard(has_file_write_intent) {
            info!("Blocked premature edit: no FILES: checklist yet");
            // Correct the turn artifact: this edit was blocked and never dispatched,
            // so it must not stay recorded as ExecutedTools (ART-EXEC-MISLABEL).
            self.write_turn_artifact(
                artifact_step,
                chat_metadata.as_ref(),
                &parsed_tool_calls_for_artifact,
                super::turn_artifacts::AgentDecision::Refused {
                    reason: "blocked: no FILES: checklist before the first edit".to_string(),
                },
                &content,
                response.reasoning_content.as_deref(),
            )
            .await;
            self.messages.push(crate::api::types::Message::user(
                "Your edit was NOT applied and has been discarded — no FILES: checklist was \
                 provided yet. First output a line `FILES: <path>` naming the file(s) you will \
                 change, then RE-ISSUE the file_edit/file_write (or file-writing shell) tool \
                 call (send it again — the previous one did not run). Do not claim the edit is \
                 done until a tool result confirms it."
                    .to_string(),
            ));
            return Ok(false);
        }

        self.execute_tool_batch(tool_calls).await?;

        // After tool batch execution, check if all tool calls were suppressed.
        // When the model keeps emitting identical tool calls that are all
        // suppressed (retry suppressed / no-op), it is stuck in tool-calling
        // mode and cannot produce a final text response on its own.
        if self.current_task_requires_mutation()
            && self.mutating_tool_call_count() > 0
            && self.consecutive_suppressions >= 3
        {
            bail!(
                "EDIT_FAILURE_LOOP_AFTER_EDIT: repeated stale edit retries after a successful mutation; stopping so the captured patch can be evaluated"
            );
        } else if self.consecutive_suppressions >= 10 {
            // After many suppressed calls, nudge the model to try a different approach.
            // Do NOT abort or force completion — the task may still need work.
            // Also clear the failed-tool cache so the agent gets fresh chances.
            info!(
                "Injecting strategy-change directive after {} consecutive suppressions — clearing failed-tool cache",
                self.consecutive_suppressions
            );
            self.consecutive_suppressions = 0; // reset so it can try again
            self.clear_failed_tool_attempts(); // give the agent a clean slate
            self.messages.push(crate::api::types::Message::user(
                "<selfware_system_directive>\n\
                 Your last several tool calls were suppressed (duplicate or no-op). \
                 This does NOT mean the task is complete. Try a DIFFERENT approach:\n\
                 - Read a different file\n\
                 - Use a different tool\n\
                 - Break the problem into smaller steps\n\
                 - If you genuinely believe the task is done, explain why in plain text.\n\
                 </selfware_system_directive>"
                    .to_string(),
            ));
        } else if self.consecutive_suppressions >= 5 {
            info!(
                "Injecting nudge after {} consecutive suppressions",
                self.consecutive_suppressions
            );
            self.messages.push(crate::api::types::Message::user(
                "<selfware_system_directive>\n\
                 Your recent tool calls were suppressed because they were duplicates or no-ops. \
                 Try a different tool or different arguments. The task is NOT necessarily complete.\n\
                 </selfware_system_directive>"
                    .to_string(),
            ));
        }

        // TERMINAL PROGRESS GUARD: After N read-only steps, force synthesis.
        // Use a relaxed threshold when the agent has already written source files —
        // verification loops (cargo check → cargo test → read output) are expected
        // after writing code and should not be punished.
        let has_any_file_write = self.has_written_any_file
            || self
                .messages
                .iter()
                .filter(|m| m.role == "assistant")
                .filter_map(|m| m.tool_calls.as_ref())
                .flatten()
                .any(|tc| matches!(tc.function.name.as_str(), "file_edit" | "file_write"));

        let terminal_threshold = if has_any_file_write { 20 } else { 8 };
        let warning_threshold = if has_any_file_write { 15 } else { 4 };

        if self.consecutive_read_only_steps >= terminal_threshold {
            self.terminal_guard_hits += 1;

            // After two terminal-guard hits without ever writing, give up.
            if self.terminal_guard_hits >= 2 && !has_any_file_write {
                info!(
                    "READ_LOOP_NO_EDIT: {} read-only steps, {} terminal-guard hits, 0 writes — aborting",
                    self.consecutive_read_only_steps, self.terminal_guard_hits
                );
                bail!(
                    "READ_LOOP_NO_EDIT: {} consecutive read-only steps without making an edit. \
                     The model was nudged twice but never modified a file.",
                    self.consecutive_read_only_steps
                );
            }

            if !has_any_file_write && self.pending_synthesis.is_some() {
                // Second time hitting terminal guard without any writes.
                // Check if src/lib.rs already has substantial content (template project).
                // If so, do NOT overwrite with scaffold — nudge targeted edits instead.
                let existing_content = tokio::fs::read_to_string("src/lib.rs")
                    .await
                    .unwrap_or_default();
                let meaningful_lines = existing_content
                    .lines()
                    .filter(|l| {
                        let t = l.trim();
                        !t.is_empty() && !t.starts_with("//") && !t.starts_with("/*")
                    })
                    .count();

                if meaningful_lines > 5 {
                    // Template project — src/lib.rs has real code. Don't overwrite.
                    info!(
                        "ESCALATED progress guard: {} read-only steps, but src/lib.rs has {} meaningful lines — nudging targeted edit instead of scaffold",
                        self.consecutive_read_only_steps, meaningful_lines
                    );
                    self.consecutive_read_only_steps = 0;
                    self.messages.push(crate::api::types::Message::user(
                        "<selfware_system_directive>\n\
                         src/lib.rs already has substantial code. Do NOT rewrite it from scratch.\n\
                         Make TARGETED edits to fix the specific bugs:\n\
                         1. Use file_edit to change only the buggy lines\n\
                         2. Keep all existing function signatures and module structure intact\n\
                         3. After editing, run cargo test to verify\n\
                         </selfware_system_directive>"
                            .to_string(),
                    ));
                    return Ok(false);
                }

                // Before assuming this is a greenfield project that needs a Rust
                // scaffold dropped at src/lib.rs, check whether the workdir is in
                // fact an existing codebase — possibly in a different language.
                // Writing a Rust stub into a Go / JS / Python repo overwrites or
                // pollutes real code and produces nonsense `git diff` output that
                // looks like progress but isn't.
                let real_codebase_evidence = count_source_files_in_workdir(5).await;
                if real_codebase_evidence > 0 {
                    info!(
                        "ESCALATED progress guard: {} read-only steps, but workdir has {} existing source file(s) outside src/lib.rs — refusing to inject Rust scaffold; nudging targeted edit",
                        self.consecutive_read_only_steps, real_codebase_evidence
                    );
                    self.consecutive_read_only_steps = 0;
                    // If we know the last file the model read, inject a concrete
                    // file_edit template instead of a generic nudge.
                    let nudge = if let Some(ref path) = self.last_read_file {
                        format!(
                            "<selfware_system_directive>\n\
                             You are working in an existing codebase that already has source files. \
                             Do NOT create a new src/lib.rs scaffold.\n\
                             You just read `{path}`. Based on your investigation, make a TARGETED edit to that file.\n\
                             Use the file_edit tool with this exact format:\n\n\
                             <tool>\n\
                             {{\"tool_type\": \"file_edit\", \"path\": \"{path}\", \"old_string\": \"...the exact original lines...\", \"new_string\": \"...the replacement lines...\"}}\n\
                             </tool>\n\n\
                             Then run the project's actual test command to verify.\n\
                             </selfware_system_directive>"
                        )
                    } else {
                        "<selfware_system_directive>\n\
                         You are working in an existing codebase that already has source files. \
                         Do NOT create a new src/lib.rs scaffold. Instead:\n\
                         1. Re-read the failing test or issue description to identify which \
                            existing function(s) need to change.\n\
                         2. Use file_edit on those specific lines — do NOT use file_write to \
                            replace whole files unless you have read them in full.\n\
                         3. Run the project's actual test command (cargo test / go test / \
                            pytest / npm test, depending on the project) to verify.\n\
                         </selfware_system_directive>"
                            .to_string()
                    };
                    self.messages.push(crate::api::types::Message::user(nudge));
                    return Ok(false);
                }

                // Greenfield project — src/lib.rs is minimal/empty AND the workdir
                // has no other source files. Write the Rust scaffold (the existing
                // SAB-style scratch-project behaviour).
                info!(
                    "ESCALATED progress guard: {} read-only steps, no writes ever — injecting scaffold",
                    self.consecutive_read_only_steps
                );
                let task = self
                    .messages
                    .iter()
                    .find(|m| m.role == "user")
                    .map(|m| m.content.to_string())
                    .unwrap_or_default();

                let scaffold = format!(
                    "// AUTO-SCAFFOLD: fill in the implementation\n\
                     // Task: {}\n\n\
                     // TODO: implement the functions described in the task\n\
                     // Then run cargo test to verify\n",
                    task.lines().next().unwrap_or("implement")
                );
                let calls: Vec<super::execution::CollectedToolCall> = vec![(
                    "file_write".to_string(),
                    serde_json::json!({"path": "src/lib.rs", "content": scaffold}).to_string(),
                    None,
                )];
                self.consecutive_read_only_steps = 0;
                self.seen_read_targets.clear();
                self.has_written_any_file = true;
                self.terminal_guard_hits = 0;
                if let Err(e) = self.execute_tool_batch(calls).await {
                    warn!("Scaffold write failed: {}", e);
                }
                self.messages.push(crate::api::types::Message::user(
                    "<selfware_system_directive>\n\
                     A scaffold file was written to src/lib.rs. Now implement the full solution:\n\
                     1. Use file_write to replace src/lib.rs with your complete implementation\n\
                     2. Include unit tests in a #[cfg(test)] mod tests block\n\
                     3. Run cargo test to verify\n\
                     </selfware_system_directive>"
                        .to_string(),
                ));
                return Ok(false);
            }

            info!(
                "TERMINAL progress guard: {} read-only steps — forcing synthesis+write",
                self.consecutive_read_only_steps
            );
            // Force immediate synthesis: extract task from first user message
            let task = self
                .messages
                .iter()
                .find(|m| m.role == "user")
                .map(|m| m.content.to_string())
                .unwrap_or_default();
            self.pending_synthesis = Some(task);
            self.consecutive_read_only_steps = 0;
            // The synthesis will fire at the top of the next step in task_runner
            return Ok(false);
        } else if self.consecutive_read_only_steps >= warning_threshold {
            info!(
                "Progress guard warning: {} read-only steps (threshold: {})",
                self.consecutive_read_only_steps, terminal_threshold
            );
            self.messages.push(crate::api::types::Message::user(format!(
                "<selfware_system_directive>\n\
                 You have spent {} consecutive steps reading without writing. \
                 You have {} steps before forced synthesis. Write code NOW:\n\n\
                 Use file_edit or file_write on an existing file you already read. \
                 Do NOT create src/lib.rs unless this repository already has Cargo.toml.\n\n\
                 <tool>\n<name>file_edit</name>\n\
                 <arguments>{{\"path\": \"PATH_YOU_ALREADY_READ\", \"old_str\": \"EXACT OLD TEXT\", \"new_str\": \"EXACT NEW TEXT\"}}</arguments>\n\
                 </tool>\n\
                 </selfware_system_directive>",
                self.consecutive_read_only_steps,
                terminal_threshold - self.consecutive_read_only_steps
            )));
        }

        Ok(false)
    }
}

// get_assistant_step_response moved to assistant_response.rs
// collect_tool_calls, message_has_tool_calls moved to tool_collect.rs
// plan moved to plan_step.rs

/// Try to extract a target file path and code content from a model's text response.
/// Public so task_runner can use it for synthesis code extraction.
/// Returns Some((path, code)) if both can be identified.
pub(super) async fn extract_code_and_path(content: &str) -> Option<(String, String)> {
    let stripped = super::recovery::strip_think_blocks(content);

    // Extract path from mentions like "src/lib.rs", "src/main.rs", etc.
    static PATH_REGEX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let path_re = PATH_REGEX.get_or_init(|| {
        regex::Regex::new(
            r#"(?:content for |file |in )?[`"]?((?:src|tests|examples)/[\w/]+\.rs)[`"]?"#,
        )
        .expect("Invalid path regex")
    });
    let explicit_path = path_re.captures(&stripped).map(|c| c[1].to_string());
    let path = if let Some(path) = explicit_path {
        path
    } else if tokio::fs::try_exists("Cargo.toml").await.unwrap_or(false) {
        // Rust-only fallback for SAB-style scratch projects.
        if stripped.contains("fn main(") {
            "src/main.rs".to_string()
        } else {
            "src/lib.rs".to_string()
        }
    } else {
        return None;
    };

    // Extract code: prefer fenced code blocks, fall back to raw code
    let code = if stripped.contains("```") {
        // Extract content between first pair of ```
        let mut in_block = false;
        let mut code_lines = Vec::new();
        for line in stripped.lines() {
            if line.starts_with("```") {
                if in_block {
                    break; // end of first block
                }
                in_block = true;
                continue;
            }
            if in_block {
                code_lines.push(line);
            }
        }
        if code_lines.len() >= 5 {
            Some(code_lines.join("\n"))
        } else {
            None
        }
    } else {
        // Extract raw code lines (Rust-like patterns)
        let code_start_patterns = [
            "use ", "pub ", "fn ", "struct ", "enum ", "impl ", "mod ", "trait ", "async ", "#[",
            "//!", "///",
        ];
        let mut code_lines = Vec::new();
        let mut collecting = false;
        for line in stripped.lines() {
            let trimmed = line.trim();
            if !collecting {
                if code_start_patterns.iter().any(|p| trimmed.starts_with(p)) {
                    collecting = true;
                    code_lines.push(line);
                }
            } else {
                // Stop collecting when we hit a blank line followed by non-code
                if trimmed.is_empty() {
                    code_lines.push(line);
                } else if trimmed.starts_with("This ")
                    || trimmed.starts_with("The ")
                    || trimmed.starts_with("Note:")
                    || trimmed.starts_with("To run")
                {
                    break; // Hit explanatory text after code
                } else {
                    code_lines.push(line);
                }
            }
        }
        if code_lines.len() >= 5 {
            Some(code_lines.join("\n"))
        } else {
            None
        }
    };

    code.map(|c| (path, c))
}

/// Detect when the model outputs code in text instead of using tools.
/// Returns true if the response contains substantial code that should have been
/// written to a file via file_write or file_edit.
/// Public so task_runner can use it for synthesis code detection.
pub(super) fn contains_unwritten_code(content: &str) -> bool {
    let stripped = super::recovery::strip_think_blocks(content);

    // Strategy 1: Check markdown fenced code blocks
    let code_block_count = stripped.matches("```").count() / 2;
    if code_block_count > 0 {
        let mut in_code_block = false;
        let mut code_lines = 0;
        for line in stripped.lines() {
            if line.starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block && !line.trim().is_empty() {
                code_lines += 1;
            }
        }
        if code_lines >= 10 {
            tracing::debug!(
                "Detected {} code lines in {} fenced code blocks",
                code_lines,
                code_block_count
            );
            return true;
        }
    }

    // Strategy 2: Detect raw unfenced code in the response.
    // Count lines that look like code (Rust-centric patterns).
    let code_indicators = [
        "pub fn ",
        "fn ",
        "pub struct ",
        "struct ",
        "impl ",
        "pub enum ",
        "enum ",
        "use ",
        "mod ",
        "#[",
        "let ",
        "pub mod ",
        "async fn ",
        "pub async fn ",
        "-> Result",
        "-> Option",
        "pub trait ",
        "trait ",
        "pub(crate)",
        "pub(super)",
    ];
    let mut raw_code_lines = 0;
    for line in stripped.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if code_indicators.iter().any(|ind| trimmed.starts_with(ind))
            || trimmed.ends_with('{')
            || trimmed == "}"
            || trimmed.ends_with(';')
        {
            raw_code_lines += 1;
        }
    }

    // Low threshold: 5+ code lines is substantial enough to auto-write
    if raw_code_lines >= 5 {
        tracing::debug!("Detected {} raw code lines in response", raw_code_lines);
        return true;
    }

    false
}

fn is_observational_shell_batch(tool_calls: &[CollectedToolCall]) -> bool {
    !tool_calls.is_empty()
        && tool_calls.iter().all(|(name, args_str, _)| {
            if name != "shell_exec" {
                return false;
            }
            let command = serde_json::from_str::<serde_json::Value>(args_str)
                .ok()
                .and_then(|v| {
                    v.get("command")
                        .and_then(|c| c.as_str())
                        .map(str::to_string)
                })
                .unwrap_or_default();
            super::tool_dispatch::shell_command_is_observational(&command)
        })
}

/// True when a tool call is a file-WRITE intent for the FILES: checklist
/// guard — i.e. it can change the CONTENT of a file the way `file_edit` /
/// `file_write` would. Non-shell tools keep the existing state-change
/// classification (file_edit/file_write/patch_apply all count; read-only
/// tools don't). `shell_exec` is classified by command: only commands that
/// write file content (redirects, tee, in-place sed/perl, cp/mv/rsync,
/// patch/git apply, cargo fmt/fix) count. Mutating-but-not-file-writing
/// commands — mkdir, touch, rm, chmod, npm/pip installs, git add/commit,
/// builds — deliberately do NOT: the guard's job is to stop blind edits,
/// and dropping an `npm install` batch for lacking a FILES: line just burns
/// turns while the model re-issues the identical command.
fn tool_call_is_file_write_intent(name: &str, args_str: &str) -> bool {
    if name != "shell_exec" {
        return super::tool_dispatch::tool_call_counts_as_state_change(name, args_str);
    }
    let command = serde_json::from_str::<serde_json::Value>(args_str)
        .ok()
        .and_then(|v| {
            v.get("command")
                .and_then(|c| c.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default();
    shell_command_is_file_write_intent(&command)
}

/// Classify a shell command by whether it writes file CONTENT. Defaults to
/// false (unlike `shell_command_is_observational`'s mutating markers, which
/// cover filesystem mutations like mkdir/rm that don't author file content).
fn shell_command_is_file_write_intent(command: &str) -> bool {
    let normalized = command.trim().to_lowercase();
    if normalized.is_empty() {
        return false;
    }

    // A redirect writes a file even without surrounding spaces (echo x>y).
    if shell_redirect_writes_file(&normalized) {
        return true;
    }

    let content_write_markers = [
        "| tee",
        " tee ",
        "sed -i",
        "sed --in-place",
        "perl -pi",
        "cp ",
        "mv ",
        "rsync ",
        "patch ",
        "git apply",
        "cargo fmt",
        "cargo fix",
    ];
    content_write_markers
        .iter()
        .any(|marker| normalized.contains(marker))
}

/// Detect an output redirect to a FILE (`>`, `>>`, `cat>file`, `echo x>y`)
/// while ignoring (a) descriptor duplication that writes no file (`2>&1`,
/// `>&2`) and (b) any `>` inside single/double quotes (`grep "->" f`).
/// Mirrors `tool_dispatch::has_file_redirect` (private there); keep the
/// two implementations in sync.
fn shell_redirect_writes_file(command: &str) -> bool {
    let chars: Vec<char> = command.chars().collect();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if escaped {
            escaped = false;
            i += 1;
            continue;
        }
        match c {
            '\\' if !in_single => escaped = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '>' if !in_single && !in_double => {
                // Skip a second '>' (append), then any spaces.
                let mut j = i + 1;
                if chars.get(j) == Some(&'>') {
                    j += 1;
                }
                while chars.get(j) == Some(&' ') {
                    j += 1;
                }
                // '>&' duplicates a descriptor (e.g. 2>&1) — no file is written.
                if chars.get(j) != Some(&'&') {
                    return true;
                }
            }
            _ => {}
        }
        i += 1;
    }
    false
}

#[cfg(test)]
#[path = "../../tests/unit/agent/execution/execution_test.rs"]
mod tests;
