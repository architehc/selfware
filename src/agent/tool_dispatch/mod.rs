use std::hash::{Hash, Hasher};

use anyhow::Result;
use colored::*;
use serde_json::Value;
use tracing::{debug, info, warn};

use super::*;
use crate::api::types::Message;
use crate::checkpoint::ToolCallLog;
use crate::cognitive::self_improvement::Outcome;
use crate::hooks::HookContext;

mod helpers;
mod spill;
mod trust_gate;

pub(crate) use helpers::*;
pub(crate) use spill::*;
pub(crate) use trust_gate::*;

impl Agent {
    /// Credit (or record the failure of) a verification tool call for the
    /// completion gate's StaleVerification check. Single accounting path for
    /// both dispatch sites: a SUCCESSFUL recognized verification call marks
    /// the current mutation sequence as verified; a FAILED one records the
    /// summary so the gate can reject with FailingTestsAccepted. Callers run
    /// this AFTER the mutating-call accounting, so a command that is both
    /// mutating and verifying (e.g. an inline `python3 -c` check) still ends
    /// the turn credited rather than stale.
    pub(super) fn note_verification_outcome(
        &mut self,
        name: &str,
        args_str: &str,
        success: bool,
        result_str: &str,
    ) {
        if success && tool_call_is_verification(name, args_str) {
            // Loop 12: a passing verification breaks any repeated-probe
            // streak — probes interleaved with green checks are iteration,
            // not a stall.
            self.probe_command_counts.clear();
            if self.mutation_sequence > 0 {
                self.last_successful_verification_mutation_sequence = self.mutation_sequence;
                self.last_failed_verification_summary = None;
            }
        } else if !success && tool_call_is_verification(name, args_str) {
            let preview: String = result_str.chars().take(300).collect();
            self.last_failed_verification_summary = Some(format!("{} failed: {}", name, preview));
        }
    }

    fn current_task_tool_policy_violation(&self, tool_name: &str) -> Option<String> {
        let task = self.learning_context();
        if task.trim().is_empty() || task == "general" {
            return None;
        }

        let disallowed = extract_explicit_disallowed_tools(task);
        if disallowed.contains(tool_name) {
            return Some(format!(
                "Task tool policy violation: `{}` is explicitly disallowed by the task instructions. Choose a different tool now.",
                tool_name
            ));
        }

        let allowed = extract_explicit_allowed_tools(task)?;
        if allowed.contains(tool_name) {
            return None;
        }

        let allowed_list = allowed
            .iter()
            .map(|tool| format!("`{}`", tool))
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!(
            "Task tool policy violation: `{}` is not allowed for this task. Allowed tools from the task prompt: {}. Use one of those tools instead.",
            tool_name, allowed_list
        ))
    }

    async fn reject_tool_call_before_execution(
        &mut self,
        tool_name: &str,
        args_str: &str,
        call_id: &str,
        use_native_fc: bool,
        start_time: std::time::Instant,
        failure_kind: &'static str,
        error_msg: &str,
    ) {
        cli_println!("{} {}", "✗".bright_red(), error_msg);
        self.pending_failure_hint = Some(error_msg.to_string());
        self.push_tool_result_message(
            use_native_fc,
            call_id,
            tool_name,
            args_str,
            false,
            error_msg,
        )
        .await;
        self.log_tool_call(tool_name, args_str, error_msg, false, start_time, false);
        self.remember_failed_tool(tool_name, error_msg);
        self.record_failed_tool_attempt(tool_name, args_str, failure_kind, error_msg);
        self.consecutive_suppressions += 1;
    }

    async fn maybe_block_progressless_batch(
        &mut self,
        tool_calls: Vec<super::execution::CollectedToolCall>,
    ) -> Result<Option<Vec<super::execution::CollectedToolCall>>> {
        // Use relaxed threshold when agent has already written source files.
        // Verification loops (cargo check → cargo test → read output) are expected
        // after writing and should not be blocked aggressively.
        let has_written = self.has_written_any_file;

        // Pre-edit thresholds are generous (12/18) so that legitimate
        // investigation of a complex change — reading many distinct files
        // before the first edit — is not prematurely blocked.  True infinite
        // loops are still caught because the investigation-progress reset
        // (in execution.rs) only rewards *novel* reads; redundant re-reads
        // let the counter climb to the threshold.
        let block_threshold = if has_written { 16 } else { 12 };
        let escalation_threshold = if has_written { 20 } else { 18 };

        if !task_requires_mutation(self.task_context_for_classification())
            || self.consecutive_read_only_steps <= block_threshold
            || tool_calls.is_empty()
            || !tool_calls
                .iter()
                .all(|(name, args_str, _)| tool_call_is_observational(name, args_str))
        {
            return Ok(Some(tool_calls));
        }

        let error_msg = format!(
            "PROGRESS GUARD: This task requires making changes, but you have already spent {} consecutive steps on read-only or verification actions. Read-only tools are temporarily blocked. Your next action must change code or project state: use `file_edit`, `file_write`, `file_delete`, or `shell_exec` with a mutating command. Do NOT rerun more reads, status commands, or test commands until after you edit something.",
            self.consecutive_read_only_steps
        );

        // Record the firing for FailureMode classification.
        self.note_progress_guard_fired();
        self.emit_progress(super::progress::ProgressEvent::GuardFired {
            kind: "progress_guard".to_string(),
            count: self.progress_guard_fire_count(),
        });
        let guard_count = self.progress_guard_fire_count();

        // Hard-abort BEFORE the per-call rejection bookkeeping: an error
        // return must not leave tool results recorded for calls that were
        // never adjudicated.
        if guard_count >= 3 && self.mutating_tool_call_count() == 0 {
            anyhow::bail!(
                "READ_LOOP_NO_EDIT: progress guard blocked read-only tools {} times after {} consecutive read-only steps, with 0 mutating tools",
                guard_count,
                self.consecutive_read_only_steps
            );
        }

        for (name, args_str, tool_call_id) in tool_calls {
            let start_time = std::time::Instant::now();
            let (call_id, use_native_fc, _) =
                self.build_tool_call_context(&name, &args_str, tool_call_id);
            self.reject_tool_call_before_execution(
                &name,
                &args_str,
                &call_id,
                use_native_fc,
                start_time,
                "progress_guard",
                &error_msg,
            )
            .await;
        }

        if self.config.agent.read_loop_policy == crate::config::ReadLoopPolicy::ForceMutation {
            self.force_mutation_pending = true;
            self.messages
                .push(Message::user(self.force_mutation_directive()));
        } else {
            self.messages.push(Message::user(
                "<selfware_system_directive>\n\
                 Read-only and verification tools are blocked until you make a real change.\n\
                 Your NEXT response must do one of these:\n\
                 - use `file_edit`, `file_write`, or `file_delete`\n\
                 - use `shell_exec` with a mutating command\n\
                 - if you already know the exact code change, output the replacement code as text and include the target path; Selfware will write it automatically\n\
                 Do NOT call more file reads, directory listings, grep searches, cargo test, or cargo check right now.\n\
                 </selfware_system_directive>"
                    .to_string(),
            ));
        }

        if self.consecutive_read_only_steps >= escalation_threshold
            && self.pending_synthesis.is_none()
        {
            info!(
                "Escalating progress-guard stall to phase-2 synthesis after {} read-only steps",
                self.consecutive_read_only_steps
            );
            self.pending_synthesis = Some(self.learning_context().to_string());
        }

        Ok(None)
    }

    fn force_mutation_directive(&self) -> String {
        let target = self
            .last_read_file
            .as_deref()
            .unwrap_or("PATH_YOU_ALREADY_READ");
        let target_json =
            serde_json::to_string(target).unwrap_or_else(|_| "\"PATH_YOU_ALREADY_READ\"".into());
        format!(
            "<selfware_system_directive>\n\
             READ-LOOP FORCE-MUTATION MODE is active.\n\
             Your previous read-only or verification tool calls were suppressed. \
             The next accepted action must mutate code or project state.\n\n\
             Choose ONE of these exact tool shapes on the most relevant existing file you already inspected:\n\n\
             <tool>\n\
             <name>file_edit</name>\n\
             <arguments>{{\"path\":{target_json},\"old_str\":\"EXACT ORIGINAL TEXT FROM THE FILE\",\"new_str\":\"REPLACEMENT TEXT\"}}</arguments>\n\
             </tool>\n\n\
             <tool>\n\
             <name>file_multi_edit</name>\n\
             <arguments>{{\"path\":{target_json},\"edits\":[{{\"old_str\":\"...\",\"new_str\":\"...\"}}]}}</arguments>\n\
             </tool>\n\n\
             <tool>\n\
             <name>file_write</name>\n\
             <arguments>{{\"path\":{target_json},\"content\":\"FULL NEW FILE CONTENT\"}}</arguments>\n\
             </tool>\n\n\
             <tool>\n\
             <name>patch_apply</name>\n\
             <arguments>{{\"patch\":\"--- a/file\\n+++ b/file\\n@@ -1 +1 @@\\n-old\\n+new\\n\"}}</arguments>\n\
             </tool>\n\n\
             If you already know the exact change, you may also output the replacement code as plain text with the target path; Selfware will write it automatically.\n\n\
             Rules:\n\
             - Do NOT call file_read, directory_tree, glob_find, grep_search, git_diff, cargo_check, cargo_test, pytest, npm test, or go test BEFORE making an edit.\n\
             - After you edit, you MAY run one targeted test command to verify the change.\n\
             - Do NOT create src/lib.rs unless this repository already has Cargo.toml and src/lib.rs is the real target.\n\
             - You MUST make an edit now. If uncertain, edit the highest-ranked source file with the smallest plausible fix; do not stop without editing.\n\
             </selfware_system_directive>"
        )
    }

    pub(super) fn push_task_state_note(&mut self, note: String) {
        if self.task_state_notes.back() == Some(&note) {
            return;
        }
        // `>=` (not `==`): any state that ever exceeds the limit — a pusher
        // without this check, or a lowered limit — self-corrects here instead
        // of growing unbounded.
        while self.task_state_notes.len() >= TASK_STATE_NOTE_LIMIT {
            self.task_state_notes.pop_front();
        }
        self.task_state_notes.push_back(note);
    }

    pub(super) fn clear_task_state_memory(&mut self) {
        self.file_tracker.read_state.clear();
        self.task_state_notes.clear();
    }

    fn remember_failed_tool(&mut self, tool_name: &str, error: &str) {
        let error_preview = truncate_chars(error, TOOL_FAILURE_HINT_PREVIEW_CHARS);

        // Classify the error and generate contextual recovery hint
        let error_kind = ToolErrorKind::classify(error);
        let recovery_hint = error_kind.recovery_hint();

        self.pending_failure_hint = Some(format!(
            "⚠️  Tool failure [{}]: `{}` failed.\n   Error: {}\n   Recovery: {}",
            error_kind.as_str(),
            tool_name,
            error_preview,
            recovery_hint
        ));
    }

    fn build_failed_tool_retry_suppressed_message(&self, failure: &FailedToolAttempt) -> String {
        let schema_hint = self
            .tools
            .get(&failure.tool_name)
            .and_then(|tool| {
                let required: Vec<String> = tool
                    .schema()
                    .get("required")
                    .and_then(|value| value.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|value| value.as_str())
                    .map(|field| format!("`{}`", field))
                    .collect();
                (!required.is_empty())
                    .then(|| format!(" Required top-level fields: {}.", required.join(", ")))
            })
            .unwrap_or_default();

        match failure.failure_kind {
            "parsing" => format!(
                "RETRY SUPPRESSED: `{}` with these exact arguments already failed because the arguments were not valid JSON.{} Change the JSON before retrying. Last error: {}",
                failure.tool_name, schema_hint, failure.error_preview
            ),
            "validation" => format!(
                "RETRY SUPPRESSED: `{}` with these exact arguments already failed schema validation.{} Change the arguments before retrying. Last error: {}",
                failure.tool_name, schema_hint, failure.error_preview
            ),
            "safety" => format!(
                "RETRY SUPPRESSED: `{}` with these exact arguments already failed the safety check. Change the tool or arguments before retrying. Last error: {}",
                failure.tool_name, failure.error_preview
            ),
            "task_policy" => format!(
                "RETRY SUPPRESSED: `{}` is blocked by the task's explicit tool constraints. Use a tool that matches the task instructions instead. Last error: {}",
                failure.tool_name, failure.error_preview
            ),
            "operator_denied" => format!(
                "RETRY SUPPRESSED: the operator denied `{}` with these exact arguments. Do not ask for the same permission again; choose a different approach or explain that the task cannot continue without it. Last response: {}",
                failure.tool_name, failure.error_preview
            ),
            "progress_guard" => format!(
                "RETRY SUPPRESSED: `{}` is blocked by the progress guard because you need to make an edit or other state-changing action before using more read-only or verification tools. Last error: {}",
                failure.tool_name, failure.error_preview
            ),
            other => {
                // For file_read failures, hint that the file may need to be created first
                let hint = if failure.tool_name == "file_read" && failure.error_preview.contains("Failed to read") {
                    " If the file does not exist yet, use file_write to CREATE it first."
                } else {
                    ""
                };
                format!(
                    "RETRY SUPPRESSED: `{}` with these exact arguments already failed due to {}. Do not rerun it until a different successful tool call changes the situation or you change the inputs.{} Last error: {}",
                    failure.tool_name, other, hint, failure.error_preview
                )
            },
        }
    }

    pub(super) fn record_failed_tool_attempt(
        &mut self,
        tool_name: &str,
        args_str: &str,
        failure_kind: &'static str,
        error: &str,
    ) {
        let args_hash = hash_tool_args(args_str);
        let error_preview = truncate_chars(error, TOOL_FAILURE_HINT_PREVIEW_CHARS);
        self.recent_failed_tool_attempts.retain(|existing| {
            !(existing.tool_name == tool_name
                && existing.args_hash == args_hash
                && existing.failure_kind == failure_kind)
        });
        self.recent_failed_tool_attempts
            .push_back(FailedToolAttempt {
                tool_name: tool_name.to_string(),
                args_hash,
                failure_kind,
                error_preview,
            });
        if self.recent_failed_tool_attempts.len() > FAILED_TOOL_ATTEMPT_WINDOW_SIZE {
            self.recent_failed_tool_attempts.pop_front();
        }
    }

    pub(super) fn clear_failed_tool_attempts(&mut self) {
        self.recent_failed_tool_attempts.clear();
        self.escalated_edit_args_hashes.clear();
        self.consecutive_suppressions = 0;
    }

    /// Record an escalated edit-args hash, FIFO-bounded at
    /// ESCALATED_EDIT_ARGS_WINDOW_SIZE so a model varying old_str/new_str on
    /// each retry cannot grow the cache without bound.
    pub(super) fn record_escalated_edit(&mut self, hash: u64) {
        if self.escalated_edit_args_hashes.contains(&hash) {
            return;
        }
        self.escalated_edit_args_hashes.push_back(hash);
        while self.escalated_edit_args_hashes.len() > ESCALATED_EDIT_ARGS_WINDOW_SIZE {
            self.escalated_edit_args_hashes.pop_front();
        }
    }

    /// Clear recorded failed attempts for a single tool name.
    /// Used when that tool succeeds so that unrelated failures are not forgiven.
    pub(super) fn clear_failed_tool_attempts_for_tool(&mut self, tool_name: &str) {
        self.recent_failed_tool_attempts
            .retain(|existing| existing.tool_name != tool_name);
    }

    /// Paths the agent wrote/edited this task (from checkpoint tool calls).
    pub(super) fn written_paths(&self) -> Vec<std::path::PathBuf> {
        self.current_checkpoint
            .as_ref()
            .map(|cp| {
                cp.tool_calls
                    .iter()
                    .filter(|tc| matches!(tc.tool_name.as_str(), "file_edit" | "file_write"))
                    .filter_map(|tc| {
                        serde_json::from_str::<serde_json::Value>(&tc.arguments)
                            .ok()
                            .and_then(|v| {
                                v.get("path")
                                    .and_then(|p| p.as_str())
                                    .map(std::path::PathBuf::from)
                            })
                    })
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Stagnation accounting (loop 13d): count consecutive tool calls that
    /// leave the workspace fingerprint unchanged and aren't a green
    /// verification. Warn once at 10, abort at 20 — a run whose workspace
    /// never moves is not converging (data-anonymization: 67 probes, 3600s).
    /// Mutation tasks only; fingerprint errors fail open (counted as changed).
    pub(super) fn note_workspace_state_with_root(
        &mut self,
        root: &std::path::Path,
        tool_name: &str,
        args_str: &str,
        success: bool,
    ) -> Result<()> {
        if !self.current_task_requires_mutation() {
            return Ok(());
        }
        let fingerprint = workspace_fingerprint(root);
        let green_verification =
            success && super::tool_dispatch::tool_call_is_verification(tool_name, args_str);
        match (fingerprint, self.last_workspace_fingerprint) {
            (Some(now), Some(prev)) if now == prev && !green_verification => {
                self.stagnation_streak += 1;
            }
            _ => {
                self.stagnation_streak = 0;
            }
        }
        if fingerprint.is_some() {
            self.last_workspace_fingerprint = fingerprint;
        }

        if self.stagnation_streak == 10
            && !self
                .stagnation_warned
                .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            self.messages.push(crate::api::types::Message::user(
                "<selfware_system_directive>\n\
                 STALL: 10 consecutive tool calls produced no workspace change and no passing \
                 verification. Your next action must change the deliverable or run the \
                 verification command.\n\
                 </selfware_system_directive>"
                    .to_string(),
            ));
        }
        if self.stagnation_streak >= 20 {
            anyhow::bail!(
                "WORKSPACE_STAGNATION: 20 consecutive tool calls with no workspace change and no \
                 passing verification — the run is not converging"
            );
        }
        Ok(())
    }

    /// Stagnation accounting at the agent's project root.
    pub(super) fn note_workspace_state(
        &mut self,
        tool_name: &str,
        args_str: &str,
        success: bool,
    ) -> Result<()> {
        let root = super::current_project_root();
        self.note_workspace_state_with_root(&root, tool_name, args_str, success)
    }

    /// Best-snapshot capture: a green verification marks the current written
    /// state as the best known (submit best state, not last state).
    pub(super) fn note_green_verification(&mut self, name: &str, args_str: &str, success: bool) {
        if !success || !super::tool_dispatch::tool_call_is_verification(name, args_str) {
            return;
        }
        let paths = self.written_paths();
        if paths.is_empty() {
            return;
        }
        match self.best_snapshot.snapshot_written(&paths) {
            Ok(()) => info!(
                "best snapshot updated after green verification ({} files)",
                paths.len()
            ),
            Err(e) => warn!("best snapshot capture failed: {e}"),
        }
    }

    /// Dependency-firewall accounting: count consecutive install failures.
    /// Resets only on a successful install — interleaved successful
    /// diagnostics are part of the spiral pattern, not progress out of it.
    pub(super) fn note_shell_outcome(&mut self, command: &str, success: bool) {
        if !is_dependency_install_command(command) {
            return;
        }
        if success {
            self.failed_install_streak = 0;
        } else {
            self.failed_install_streak += 1;
        }
    }

    /// Block an install command when the install streak has hit the limit —
    /// the environment is not yielding, so the model must pivot (stdlib-only,
    /// vendored, different tool) instead of retrying the same ladder forever.
    /// Returns true when the call was rejected.
    pub(super) async fn maybe_block_dependency_spiral(
        &mut self,
        tool_name: &str,
        args_str: &str,
        call_id: &str,
        use_native_fc: bool,
        start_time: std::time::Instant,
    ) -> bool {
        if !matches!(tool_name, "shell_exec" | "pty_shell") {
            return false;
        }
        if self.failed_install_streak < DEPENDENCY_SPIRAL_LIMIT {
            return false;
        }
        let Some(command) = serde_json::from_str::<serde_json::Value>(args_str)
            .ok()
            .and_then(|v| v.get("command").and_then(|c| c.as_str()).map(String::from))
        else {
            return false;
        };
        if !is_dependency_install_command(&command) {
            return false;
        }

        let directive = format!(
            "DEPENDENCY FIREWALL: {} consecutive install attempts have failed (latest: `{command}`). \
             The environment is not yielding — do NOT retry installation. Pivot now: \
             (a) use a stdlib-only approach, (b) vendor a minimal implementation, \
             (c) use a different tool that is already installed, or (d) state exactly which \
             package+version is missing and continue the task assuming it. The streak resets \
             only on a successful install.",
            self.failed_install_streak
        );
        self.push_tool_result_message(
            use_native_fc,
            call_id,
            tool_name,
            args_str,
            false,
            &directive,
        )
        .await;
        self.log_tool_call(tool_name, args_str, &directive, false, start_time, false);
        self.consecutive_suppressions += 1;
        true
    }

    /// Repeated-probe pivot (loop 12): the same normalized shell command
    /// (digits/whitespace collapsed) more than REPEATED_PROBE_LIMIT times with
    /// no intervening successful verification means the probe is exhausted —
    /// block the next identical call ONCE with a change-strategy directive.
    /// The latch caps this at one fire per task; counting itself is fail-open
    /// (non-shell tools, unparseable args, and an over-cap tracking map all
    /// pass through). Returns true when the call was rejected.
    pub(super) async fn maybe_block_repeated_probe(
        &mut self,
        tool_name: &str,
        args_str: &str,
        call_id: &str,
        use_native_fc: bool,
        start_time: std::time::Instant,
    ) -> bool {
        if !matches!(tool_name, "shell_exec" | "pty_shell") {
            return false;
        }
        if self
            .probe_pivot_done
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return false;
        }
        let Some(command) = serde_json::from_str::<serde_json::Value>(args_str)
            .ok()
            .and_then(|v| v.get("command").and_then(|c| c.as_str()).map(String::from))
        else {
            return false;
        };

        let normalized = normalize_probe_command(&command);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        normalized.hash(&mut hasher);
        let probe_hash = hasher.finish();
        let count = match self.probe_command_counts.get_mut(&probe_hash) {
            Some(count) => {
                *count += 1;
                *count
            }
            None => {
                if self.probe_command_counts.len() >= TRACKED_PROBE_COMMAND_LIMIT {
                    // Fail-open: too many distinct commands to track — never block.
                    return false;
                }
                self.probe_command_counts.insert(probe_hash, 1);
                1
            }
        };
        if count <= REPEATED_PROBE_LIMIT {
            return false;
        }

        // Latch BEFORE the bookkeeping so no retry path can re-fire it.
        self.probe_pivot_done
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let preview = truncate_chars(&command, TOOL_CONFIRM_ARGS_PREVIEW_CHARS);
        let directive = format!(
            "<selfware_system_directive>\n\
             REPEATED PROBE PIVOT: the same command (digits/whitespace normalized) has now run \
             {count} times with no successful verification in between (latest: `{preview}`). \
             The approach is exhausted — rerunning it will not produce a different result. \
             Change strategy NOW: (a) gather the missing information with a materially \
             different command or tool, or (b) stop probing and write the final artifact now \
             from what you already know, then run the real verification command once.\n\
             </selfware_system_directive>"
        );
        self.push_tool_result_message(
            use_native_fc,
            call_id,
            tool_name,
            args_str,
            false,
            &directive,
        )
        .await;
        self.log_tool_call(tool_name, args_str, &directive, false, start_time, false);
        self.consecutive_suppressions += 1;
        true
    }

    pub(super) async fn maybe_block_redundant_reread(
        &mut self,
        name: &str,
        args_str: &str,
        args: &Value,
        call_id: &str,
        use_native_fc: bool,
        start_time: std::time::Instant,
    ) -> bool {
        if name != "file_read" {
            return false;
        }

        let Some(path) = args.get("path").and_then(|v| v.as_str()) else {
            return false;
        };
        let Some(state) = self.file_tracker.read_state.get(path) else {
            return false;
        };
        // Allow up to 3 unchanged rereads before blocking — in long sessions
        // the model may need to re-read files after context compression evicts
        // earlier content. Only block truly excessive rereads.
        if state.unchanged_read_count < 3 || self.file_tracker.stale_files.contains(path) {
            return false;
        }

        let current_mtime = tokio::fs::metadata(path)
            .await
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs());

        if current_mtime != state.last_modified {
            return false;
        }

        // The threshold is 3 unchanged rereads; this call is the one that exceeds
        // it, so the ordinal is one greater than the threshold.
        let read_count = state.unchanged_read_count + 1;
        // Increment the counter so repeated suppressions eventually trigger
        // the forced text response (at count >= 3).
        if let Some(state_mut) = self.file_tracker.read_state.get_mut(path) {
            state_mut.unchanged_read_count = read_count;
        }
        let err = format!(
            "Repeated unchanged reread blocked: `{}` has already been read unchanged 3 times in this task. Use the content already in context or make the edit now instead of reading it again.",
            path
        );
        self.push_task_state_note(format!(
            "Blocked redundant reread of `{}` on the {}th unchanged read",
            path, read_count
        ));
        self.pending_failure_hint = Some(err.clone());
        self.push_tool_result_message(use_native_fc, call_id, name, args_str, false, &err)
            .await;
        self.log_tool_call(name, args_str, &err, false, start_time, false);
        self.remember_failed_tool(name, &err);
        self.record_failed_tool_attempt(name, args_str, "task_state", &err);
        self.consecutive_suppressions += 1;

        // After suppressed rereads, trigger phase-2 synthesis early.
        // The model has the data in context — force it to produce code.
        if read_count >= 3 && self.pending_synthesis.is_none() {
            info!(
                "Triggering phase-2 synthesis after {} suppressed rereads",
                read_count
            );
            // Extract the task description from the first user message
            let task = self
                .messages
                .iter()
                .find(|m| m.role == "user")
                .map(|m| m.content.to_string())
                .unwrap_or_default();
            self.pending_synthesis = Some(task);
        }

        true
    }

    pub(super) async fn track_task_state_after_tool(
        &mut self,
        name: &str,
        args: &Value,
        result: &str,
        success: bool,
    ) {
        if !success {
            return;
        }

        let Some(path) = args.get("path").and_then(|v| v.as_str()) else {
            return;
        };
        let path_str = path.to_string();

        match name {
            "file_read" => {
                let Ok(json) = serde_json::from_str::<Value>(result) else {
                    return;
                };
                let Some(content) = json.get("content").and_then(|v| v.as_str()) else {
                    return;
                };
                let total_lines = json
                    .get("total_lines")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                let content_hash = super::recovery::hash_text_signature(content);
                let last_modified = tokio::fs::metadata(&path_str)
                    .await
                    .ok()
                    .and_then(|metadata| metadata.modified().ok())
                    .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_secs());

                let mut unchanged_count = 0;
                if let Some(state) = self.file_tracker.read_state.get_mut(&path_str) {
                    if state.content_hash == content_hash
                        && state.last_modified == last_modified
                        && !self.file_tracker.stale_files.contains(&path_str)
                    {
                        state.unchanged_read_count += 1;
                        unchanged_count = state.unchanged_read_count;
                    } else {
                        state.content_hash = content_hash;
                        state.total_lines = total_lines;
                        state.last_modified = last_modified;
                        state.unchanged_read_count = 0;
                    }
                } else {
                    self.file_tracker.read_state.insert(
                        path_str.clone(),
                        FileReadState {
                            content_hash,
                            total_lines,
                            last_modified,
                            unchanged_read_count: 0,
                        },
                    );
                }

                if unchanged_count > 0 {
                    self.push_task_state_note(format!(
                        "Reread unchanged file `{}` ({}x consecutive unchanged reads)",
                        path_str, unchanged_count
                    ));
                }

                if unchanged_count >= 1 {
                    self.pending_failure_hint = Some(format!(
                        "You have reread unchanged file `{}` {} times in this task. Unless something outside the agent changed it, use the content already in context or make the edit now instead of reading it again.",
                        path_str, unchanged_count
                    ));
                }
            }
            "file_write" | "file_edit" => {
                self.file_tracker.mark_written(&path_str);
                self.push_task_state_note(format!(
                    "Marked `{}` as changed; future rereads should expect new content",
                    path_str
                ));
            }
            "file_delete" => {
                self.file_tracker.remove_deleted(&path_str);
                self.push_task_state_note(format!(
                    "Removed deleted file `{}` from task-state tracking",
                    path_str
                ));
            }
            _ => {}
        }
    }

    pub(super) async fn suppress_repeated_failed_tool_retry(
        &mut self,
        tool_name: &str,
        args_str: &str,
        call_id: &str,
        use_native_fc: bool,
        start_time: std::time::Instant,
    ) -> bool {
        let args_hash = hash_tool_args(args_str);
        let Some(failure) = self
            .recent_failed_tool_attempts
            .iter()
            .rev()
            .find(|attempt| attempt.tool_name == tool_name && attempt.args_hash == args_hash)
            .cloned()
        else {
            return false;
        };

        // For file_read failures, check if the file exists now — it may have
        // been created by file_write since the last failed attempt. Only a
        // confirmed-absent file stays suppressed: a stat error (permissions,
        // transient I/O) must not masquerade as "does not exist".
        if tool_name == "file_read" {
            if let Ok(args) = serde_json::from_str::<serde_json::Value>(args_str) {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    let exists = tokio::fs::try_exists(path).await;
                    if !file_read_retry_stays_suppressed(&exists) {
                        info!(
                            "file_read('{}') was previously suppressed but file is now readable — allowing retry",
                            path
                        );
                        self.recent_failed_tool_attempts
                            .retain(|a| !(a.tool_name == tool_name && a.args_hash == args_hash));
                        return false;
                    }
                }
            }
        }

        // For file_edit failures (old_str not found), escalate the workflow:
        // 1. Force a file_read of the target so the model sees current content
        // 2. Tell the model to use file_write instead of file_edit
        // This prevents the 315-retry death spiral from the logs.
        if tool_name == "file_edit" {
            if let Ok(args) = serde_json::from_str::<serde_json::Value>(args_str) {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    // If this exact edit was already escalated once, suppress the
                    // repeat WITHOUT re-reading and re-injecting the whole file —
                    // the model already has the content (EDIT-RETRY-REINJECT).
                    if self.escalated_edit_args_hashes.contains(&args_hash) {
                        let short = format!(
                            "<selfware_system_directive>\n\
                             file_edit for {} keeps failing (old_str not found) and you were \
                             already given the full file content. Use file_write to replace the \
                             ENTIRE file now — do not retry file_edit.\n\
                             </selfware_system_directive>",
                            path
                        );
                        self.push_tool_result_message(
                            use_native_fc,
                            call_id,
                            tool_name,
                            args_str,
                            false,
                            &short,
                        )
                        .await;
                        self.log_tool_call(
                            tool_name,
                            args_str,
                            "edit_reescalation_suppressed",
                            false,
                            start_time,
                            false,
                        );
                        self.consecutive_suppressions += 1;
                        return true;
                    }
                    self.record_escalated_edit(args_hash);

                    let edit_fail_count = self
                        .recent_failed_tool_attempts
                        .iter()
                        .filter(|a| {
                            a.tool_name == "file_edit" && a.error_preview.contains("not found")
                        })
                        .count();

                    info!(
                        "file_edit failed on '{}' ({} prior edit failures) — escalating to file_write",
                        path, edit_fail_count
                    );

                    // Force-read the file so the model sees current content.
                    // Read directly (no try_exists pre-check) so a stat error
                    // can't masquerade as a missing file; cap the injection at
                    // ESCALATION_CONTENT_CHAR_BUDGET so large targets can't
                    // bloat the message history without bound.
                    let read_result = match tokio::fs::read_to_string(path).await {
                        Ok(content) => {
                            let lines = content.lines().count();
                            if content.chars().count() > ESCALATION_CONTENT_CHAR_BUDGET {
                                let kept = truncate_chars(&content, ESCALATION_CONTENT_CHAR_BUDGET);
                                let kept_lines = kept.lines().count();
                                format!(
                                    "Current content of {} ({} lines, truncated to the first {} — over the {}-char escalation budget):\n{}",
                                    path,
                                    lines,
                                    kept_lines,
                                    ESCALATION_CONTENT_CHAR_BUDGET,
                                    kept
                                )
                            } else {
                                format!(
                                    "Current content of {} ({} lines):\n{}",
                                    path, lines, content
                                )
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                            format!("File {} does not exist. Use file_write to create it.", path)
                        }
                        Err(e) => format!("Could not read {}: {}", path, e),
                    };

                    let escalation = format!(
                        "<selfware_system_directive>\n\
                         file_edit FAILED because old_str was not found in the file.\n\
                         {}\n\n\
                         DO NOT retry file_edit. Use file_write to REPLACE THE ENTIRE FILE:\n\n\
                         <tool>\n<name>file_write</name>\n\
                         <arguments>{{\"path\": \"{}\", \"content\": \"FULL FILE CONTENT HERE\"}}</arguments>\n\
                         </tool>\n\
                         </selfware_system_directive>",
                        read_result, path
                    );
                    self.push_tool_result_message(
                        use_native_fc,
                        call_id,
                        tool_name,
                        args_str,
                        false,
                        &escalation,
                    )
                    .await;
                    self.log_tool_call(
                        tool_name,
                        args_str,
                        "escalated_to_file_write",
                        false,
                        start_time,
                        false,
                    );
                    self.consecutive_suppressions += 1;
                    return true;
                }
            }
        }

        let err = self.build_failed_tool_retry_suppressed_message(&failure);
        warn!(
            "Suppressing repeated failed tool call for '{}' after prior {} failure",
            tool_name, failure.failure_kind
        );
        cli_println!("{} {}", "✗".bright_red(), err);
        self.push_tool_result_message(use_native_fc, call_id, tool_name, args_str, false, &err)
            .await;
        self.log_tool_call(tool_name, args_str, &err, false, start_time, false);
        self.remember_failed_tool(tool_name, &err);
        // Surface this as a permanently-blocked tool call for FailureMode.
        self.note_permanently_blocked(tool_name);
        let duration_ms = start_time.elapsed().as_millis() as u64;
        self.self_improvement.record_tool(
            tool_name,
            self.learning_context(),
            Outcome::Failure,
            duration_ms,
            Some(err.clone()),
        );
        self.self_improvement.record_error(
            &err,
            "retry_suppressed",
            self.learning_context(),
            tool_name,
            None,
        );
        self.consecutive_suppressions += 1;
        true
    }

    /// Tools that are safe to execute concurrently (read-only, no side effects).
    const PARALLEL_SAFE_TOOLS: &'static [&'static str] = &[
        "file_read",
        "directory_tree",
        "glob_find",
        "grep_search",
        "symbol_search",
        "git_status",
        "git_diff",
        "git_log",
        "lsp_document_symbols",
        "lsp_find_references",
        "lsp_goto_definition",
        "lsp_hover",
    ];

    /// Check if a tool can be executed concurrently with other tools.
    fn is_parallel_safe(name: &str) -> bool {
        Self::PARALLEL_SAFE_TOOLS.contains(&name)
    }

    pub(super) async fn execute_tool_batch(
        &mut self,
        tool_calls: Vec<super::execution::CollectedToolCall>,
    ) -> Result<()> {
        // Central hard-budget enforcement: the assistant response, planning, or
        // synthesis call that produced these tool calls was billable. Enforce
        // the token/cost/wall caps HERE — before ANY tool (model-requested,
        // auto-write, fallback, or scaffold) runs — so an over-budget turn
        // cannot mutate the project between the outer per-iteration checks.
        // (Under the default config there is no cap, so this is a no-op.)
        let task_desc = self
            .current_checkpoint
            .as_ref()
            .map(|cp| cp.task_description.clone())
            .unwrap_or_default();
        self.enforce_hard_budgets(&task_desc).await?;

        let Some(tool_calls) = self.maybe_block_progressless_batch(tool_calls).await? else {
            return Ok(());
        };

        // Phase 1: Partition into parallel-safe and sequential groups.
        // Read-only tools with no path conflicts go into the parallel batch.
        let mut parallel_batch: Vec<super::execution::CollectedToolCall> = Vec::new();
        let mut sequential_batch: Vec<super::execution::CollectedToolCall> = Vec::new();
        let mut parallel_paths: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        // Track whether any parallel-safe tool appears AFTER a sequential one in
        // the original order. Hoisting the parallel batch ahead of the sequential
        // batch is only safe when all parallel tools precede all sequential ones;
        // otherwise a read could run before the mutation it depends on.
        let mut parallel_follows_sequential = false;
        for call in &tool_calls {
            let (name, args_str, _) = call;
            if Self::is_parallel_safe(name) {
                // Check for path conflicts within the parallel batch
                let path = serde_json::from_str::<serde_json::Value>(args_str)
                    .ok()
                    .and_then(|v| v.get("path").and_then(|p| p.as_str()).map(String::from));
                let has_conflict = path.as_ref().is_some_and(|p| parallel_paths.contains(p));
                if has_conflict {
                    sequential_batch.push(call.clone());
                } else {
                    if let Some(ref p) = path {
                        parallel_paths.insert(p.clone());
                    }
                    if !sequential_batch.is_empty() {
                        parallel_follows_sequential = true;
                    }
                    parallel_batch.push(call.clone());
                }
            } else {
                sequential_batch.push(call.clone());
            }
        }

        // Phase 2: If fewer than 2 parallel tools, OR a parallel tool follows a
        // sequential one in the original order (so hoisting would reorder a
        // dependency), run everything sequentially in the original order to
        // preserve execution semantics.
        if parallel_batch.len() < 2 || parallel_follows_sequential {
            for (name, args_str, tool_call_id) in tool_calls {
                if self.is_cancelled() {
                    break;
                }
                // Clone name/tool_call_id before execute_single_tool_in_batch
                // takes them by value — we need them in the catch to push a
                // synthetic error result if the fn returns Err BEFORE pushing
                // any tool-result (e.g. confirmation rejection, pre-execution
                // safety gate). Without this, native-FC history gets N calls
                // but k<N results → 400.
                let name_clone = name.clone();
                let args_str_clone = args_str.clone();
                let id_clone = tool_call_id.clone();
                if let Err(e) = self
                    .execute_single_tool_in_batch(name, args_str, tool_call_id)
                    .await
                {
                    if super::task_runner::is_fatal_loop_error(&e) {
                        return Err(e);
                    }
                    // Non-fatal tool error that returned Err before pushing a
                    // tool-result: push a synthetic error result for this
                    // tool_call_id so every call gets exactly one result.
                    let (call_id, use_native_fc, _) =
                        self.build_tool_call_context(&name_clone, &args_str_clone, id_clone);
                    let error_text = e.to_string();
                    self.push_tool_result_message(
                        use_native_fc,
                        &call_id,
                        &name_clone,
                        &args_str_clone,
                        false,
                        &error_text,
                    )
                    .await;
                    warn!("Non-fatal tool error in sequential batch: {e}");
                }
            }
            return Ok(());
        }

        // Phase 3: Execute parallel-safe tools concurrently.
        debug!(
            "Executing {} tools in parallel, {} sequentially",
            parallel_batch.len(),
            sequential_batch.len()
        );
        self.execute_parallel_tools(parallel_batch).await?;

        // Phase 4: Execute sequential tools one at a time.
        for (name, args_str, tool_call_id) in sequential_batch {
            if self.is_cancelled() {
                break;
            }
            // Clone before execute_single_tool_in_batch takes them by value.
            let name_clone = name.clone();
            let args_str_clone = args_str.clone();
            let id_clone = tool_call_id.clone();
            if let Err(e) = self
                .execute_single_tool_in_batch(name, args_str, tool_call_id)
                .await
            {
                if super::task_runner::is_fatal_loop_error(&e) {
                    return Err(e);
                }
                // Push a synthetic error result so native-FC history stays
                // balanced (N calls → N results).
                let (call_id, use_native_fc, _) =
                    self.build_tool_call_context(&name_clone, &args_str_clone, id_clone);
                let error_text = e.to_string();
                self.push_tool_result_message(
                    use_native_fc,
                    &call_id,
                    &name_clone,
                    &args_str_clone,
                    false,
                    &error_text,
                )
                .await;
                warn!("Non-fatal tool error in sequential batch (phase 4): {e}");
            }
        }

        Ok(())
    }

    /// Execute multiple read-only tools concurrently.
    ///
    /// Pre-validates all tools sequentially (fast), spawns concurrent executions
    /// for tools that pass validation, then processes results sequentially.
    async fn execute_parallel_tools(
        &mut self,
        tool_calls: Vec<super::execution::CollectedToolCall>,
    ) -> Result<()> {
        use super::tui_events::AgentEvent;
        use crate::hooks::HookAction;

        // Pre-validate all tools and collect validated ones for concurrent execution
        struct ValidatedTool {
            name: String,
            args_str: String,
            args: Value,
            call_id: String,
            use_native_fc: bool,
            start_time: std::time::Instant,
        }

        let mut validated: Vec<ValidatedTool> = Vec::with_capacity(tool_calls.len());

        for (name, args_str, tool_call_id) in tool_calls {
            if self.is_cancelled() {
                break;
            }
            let args_str = inject_runtime_tool_defaults(&self.config, &name, &args_str);

            let start_time = std::time::Instant::now();
            if let Some(warning) = self
                .self_improvement
                .check_for_errors(&name, self.learning_context())
                .into_iter()
                .next()
                .filter(|w| w.likelihood >= 0.7)
            {
                warn!(
                    "Self-improvement warning before {}: potential {} pattern ({}%)",
                    name,
                    warning.error_type,
                    (warning.likelihood * 100.0) as u32
                );
            }

            let (call_id, use_native_fc, fake_call) =
                self.build_tool_call_context(&name, &args_str, tool_call_id);

            if self
                .suppress_repeated_failed_tool_retry(
                    &name,
                    &args_str,
                    &call_id,
                    use_native_fc,
                    start_time,
                )
                .await
            {
                self.emit_event(AgentEvent::ToolCompleted {
                    name: name.clone(),
                    success: false,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                });
                continue;
            }

            if self
                .maybe_block_dependency_spiral(
                    &name,
                    &args_str,
                    &call_id,
                    use_native_fc,
                    start_time,
                )
                .await
            {
                self.emit_event(AgentEvent::ToolCompleted {
                    name: name.clone(),
                    success: false,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                });
                continue;
            }

            if self
                .maybe_block_repeated_probe(&name, &args_str, &call_id, use_native_fc, start_time)
                .await
            {
                self.emit_event(AgentEvent::ToolCompleted {
                    name: name.clone(),
                    success: false,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                });
                continue;
            }

            if let Some(error_msg) = self.current_task_tool_policy_violation(&name) {
                self.reject_tool_call_before_execution(
                    &name,
                    &args_str,
                    &call_id,
                    use_native_fc,
                    start_time,
                    "task_policy",
                    &error_msg,
                )
                .await;
                self.emit_event(AgentEvent::ToolCompleted {
                    name: name.clone(),
                    success: false,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                });
                continue;
            }

            if let Err(e) = self.safety.check_tool_call(&fake_call) {
                let error_msg = format!("Safety check failed: {}", e);
                crate::output::safety_blocked(&error_msg);
                if let Some(ref logger) = self.audit_logger {
                    logger.log_safety_block(&name, &error_msg);
                }
                self.push_tool_result_message(
                    use_native_fc,
                    &call_id,
                    &name,
                    &args_str,
                    false,
                    &error_msg,
                )
                .await;
                self.log_tool_call(&name, &args_str, &error_msg, false, start_time, false);
                self.remember_failed_tool(&name, &error_msg);
                self.record_failed_tool_attempt(&name, &args_str, "safety", &error_msg);
                continue;
            }

            // Schema validation for native function calls
            if use_native_fc {
                let defs = self.tools.definitions();
                if let Err(e) = crate::agent::tool_validator::validate_tool_call(&fake_call, &defs)
                {
                    let error_msg = format!("Tool call validation failed: {}", e);
                    warn!("{}", error_msg);
                    self.push_tool_result_message(
                        use_native_fc,
                        &call_id,
                        &name,
                        &args_str,
                        false,
                        &error_msg,
                    )
                    .await;
                    self.log_tool_call(&name, &args_str, &error_msg, false, start_time, false);
                    self.remember_failed_tool(&name, &error_msg);
                    self.record_failed_tool_attempt(&name, &args_str, "validation", &error_msg);
                    self.emit_event(crate::agent::AgentEvent::ToolCompleted {
                        name: name.clone(),
                        success: false,
                        duration_ms: start_time.elapsed().as_millis() as u64,
                    });
                    continue;
                }
            }

            let args = match self
                .parse_tool_args(&name, &args_str, &call_id, use_native_fc, start_time)
                .await
            {
                Some(args) => args,
                None => continue,
            };

            if !self
                .validate_tool_args(&name, &args_str, &args, &call_id, use_native_fc, start_time)
                .await
            {
                continue;
            }

            if self
                .maybe_block_redundant_reread(
                    &name,
                    &args_str,
                    &args,
                    &call_id,
                    use_native_fc,
                    start_time,
                )
                .await
            {
                continue;
            }

            // Same gate the sequential path runs (YOLO forbidden-ops/protected-path/
            // container-mount checks, plus a confirmation prompt for anything that
            // still needs one). Without this, tools in the parallel-safe list were
            // silently exempt from the YOLO gate entirely -- e.g. a file_read of a
            // YOLO-protected path would be Block-ed in the sequential path but ran
            // unchecked here just because it happened to land in a >=2-tool batch.
            if !self
                .confirm_tool_execution(&name, &args_str, &call_id, use_native_fc)
                .await?
            {
                continue;
            }

            // Fire PreToolUse hooks (may skip execution)
            let pre_ctx = HookContext::pre_tool(&name, &args_str);
            if let HookAction::Skip { reason } = self.hook_registry.fire(&pre_ctx).await {
                let skip_msg = format!("Tool skipped by PreToolUse hook: {}", reason);
                info!("{}", skip_msg);
                self.push_tool_result_message(
                    use_native_fc,
                    &call_id,
                    &name,
                    &args_str,
                    false,
                    &skip_msg,
                )
                .await;
                continue;
            }

            self.emit_event(AgentEvent::ToolStarted { name: name.clone() });

            validated.push(ValidatedTool {
                name,
                args_str,
                args,
                call_id,
                use_native_fc,
                start_time,
            });
        }

        if validated.is_empty() {
            return Ok(());
        }

        let timeout_secs = self.config.agent.step_timeout_secs.max(1);
        let batch_cancel = self.cancel_token();

        for vt in &validated {
            let activity = crate::output::tool_activity_message(&vt.name, &vt.args);
            cli_println!("  {} {}", "↪".bright_black(), activity.dimmed());
        }

        // Execute all validated tools concurrently using the tool registry
        let mut results: Vec<(usize, (bool, String, String))> = Vec::with_capacity(validated.len());

        {
            use futures::stream::{FuturesUnordered, StreamExt};
            let mut futures = FuturesUnordered::new();

            for (idx, vt) in validated.iter().enumerate() {
                let tool_name = vt.name.clone();
                let tool_args = vt.args.clone();
                let tool_ref = self.tools.get(&tool_name);
                let cancel = batch_cancel.clone();

                futures.push(async move {
                    let Some(tool) = tool_ref else {
                        let msg = format!("Unknown tool: {}", tool_name);
                        return (idx, (false, msg.clone(), msg));
                    };
                    let start = std::time::Instant::now();
                    let execution = run_tool_bounded(
                        crate::observability::telemetry::track_tool_execution(&tool_name, || {
                            tool.execute(tool_args.clone())
                        }),
                        std::time::Duration::from_secs(timeout_secs),
                        cancel,
                    )
                    .await;
                    let elapsed = start.elapsed().as_millis() as u64;
                    match execution {
                        Ok(Ok(result)) => {
                            let tool_success = tool_result_value_indicates_success(&result);
                            let result_str =
                                serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
                            let summary = crate::output::semantic_summary(
                                &tool_name,
                                &tool_args,
                                Some(&result_str),
                                tool_success,
                                elapsed,
                            );
                            (idx, (tool_success, result_str, summary))
                        }
                        Ok(Err(e)) => {
                            let summary = crate::output::semantic_summary(
                                &tool_name,
                                &tool_args,
                                Some(&e.to_string()),
                                false,
                                elapsed,
                            );
                            (idx, (false, e.to_string(), summary))
                        }
                        Err(ToolHalt::TimedOut) => {
                            let msg = format!("Tool execution timed out after {}s", timeout_secs);
                            (idx, (false, msg.clone(), msg))
                        }
                        Err(ToolHalt::Cancelled) => {
                            let msg = format!("Tool '{}' cancelled", tool_name);
                            (idx, (false, msg.clone(), msg))
                        }
                    }
                });
            }

            while let Some(result) = futures.next().await {
                results.push(result);
            }
        }

        // Sort by original order to maintain deterministic message ordering
        results.sort_by_key(|(idx, _)| *idx);

        // Post-process all results
        for (idx, (success, result_str, summary)) in results {
            let vt = &validated[idx];

            let duration_ms = vt.start_time.elapsed().as_millis() as u64;
            self.emit_event(AgentEvent::ToolCompleted {
                name: vt.name.clone(),
                success,
                duration_ms,
            });

            if success {
                cli_println!("  {} {}", "✔".bright_green(), summary);
            } else {
                cli_println!("  {} {}", "✗".bright_red(), summary);
            }

            // Store for progressive disclosure via /last
            {
                let exit_code = serde_json::from_str::<serde_json::Value>(&result_str)
                    .ok()
                    .and_then(|v| v.get("exit_code").and_then(|c| c.as_i64()))
                    .map(|c| c as i32);
                self.store_last_tool_output(crate::agent::last_tool::LastToolOutput {
                    tool_name: vt.name.clone(),
                    summary: summary.clone(),
                    full_output: result_str.clone(),
                    success,
                    exit_code,
                    duration_ms,
                });
            }

            let tool_outcome = if success {
                Outcome::Success
            } else {
                Outcome::Failure
            };
            let tool_error = (!success).then(|| result_str.clone());
            self.self_improvement.record_tool(
                &vt.name,
                self.learning_context(),
                tool_outcome,
                duration_ms,
                tool_error.clone(),
            );
            if let Some(error_text) = tool_error {
                self.self_improvement.record_error(
                    &error_text,
                    Self::classify_error_type(&error_text),
                    self.learning_context(),
                    &vt.name,
                    None,
                );
            }
            if success {
                // Only forgive failures for the tool that actually succeeded.
                // Clearing the entire history on any success would mask unrelated
                // failures in the same parallel batch.
                self.clear_failed_tool_attempts_for_tool(&vt.name);
            } else {
                self.record_failed_tool_attempt(&vt.name, &vt.args_str, "execution", &result_str);
            }

            // Dependency-firewall accounting (loop 9).
            if matches!(vt.name.as_str(), "shell_exec" | "pty_shell") {
                if let Some(cmd) = vt.args.get("command").and_then(|c| c.as_str()) {
                    self.note_shell_outcome(cmd, success);
                }
            }
            // Best-snapshot capture on green verification.
            self.note_green_verification(&vt.name, &vt.args_str, success);
            // Stagnation accounting (loop 13d).
            self.note_workspace_state(&vt.name, &vt.args_str, success)?;

            self.track_task_state_after_tool(&vt.name, &vt.args, &result_str, success)
                .await;

            if success && tool_call_is_mutating(&vt.name, &vt.args) {
                self.note_mutating_tool_call();
                if matches!(
                    vt.name.as_str(),
                    "file_edit"
                        | "file_write"
                        | "file_fim_edit"
                        | "file_multi_edit"
                        | "patch_apply"
                ) {
                    self.has_written_any_file = true;
                    self.terminal_guard_hits = 0;
                }
            }

            self.note_verification_outcome(&vt.name, &vt.args_str, success, &result_str);

            // Track file operations for context management
            if success {
                if let Some(path) = vt.args.get("path").and_then(|v| v.as_str()) {
                    let path_str = path.to_string();
                    if vt.name == "file_read" {
                        if self.file_tracker.context_files.len() < 500
                            && !self.file_tracker.context_files.contains(&path_str)
                        {
                            self.file_tracker.context_files.push(path_str.clone());
                        }
                        if let Some(content) =
                            serde_json::from_str::<serde_json::Value>(&result_str)
                                .ok()
                                .and_then(|v| {
                                    v.get("content").and_then(|c| c.as_str()).map(String::from)
                                })
                        {
                            self.track_file_read_in_context_map(&path_str, &content)
                                .await;
                        }
                    }
                }
            }

            self.push_tool_result_message(
                vt.use_native_fc,
                &vt.call_id,
                &vt.name,
                &vt.args_str,
                success,
                &result_str,
            )
            .await;
            if !success {
                self.remember_failed_tool(&vt.name, &result_str);
            }

            self.reset_no_action_prompt_state();

            if !success {
                let recovery_hint = self.build_error_recovery_hint(&vt.name, &result_str);
                self.messages.push(Message::user(recovery_hint));
            }

            // Fire PostToolUse hooks
            let post_ctx = HookContext::post_tool(&vt.name, &vt.args_str, success, &result_str);
            self.hook_registry.fire(&post_ctx).await;

            // Audit log
            if let Some(ref logger) = self.audit_logger {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                vt.args_str.hash(&mut hasher);
                let args_hash = format!("{:x}", hasher.finish());
                logger.log_tool_execution(&vt.name, &args_hash, success, duration_ms, None);
            }

            self.log_tool_call(
                &vt.name,
                &vt.args_str,
                &result_str,
                success,
                vt.start_time,
                true,
            );
        }

        Ok(())
    }

    /// Execute a single tool call within a batch (sequential path).
    async fn execute_single_tool_in_batch(
        &mut self,
        name: String,
        args_str: String,
        tool_call_id: Option<String>,
    ) -> Result<()> {
        use super::tui_events::AgentEvent;
        use crate::hooks::HookAction;

        let start_time = std::time::Instant::now();
        if let Some(warning) = self
            .self_improvement
            .check_for_errors(&name, self.learning_context())
            .into_iter()
            .next()
            .filter(|w| w.likelihood >= 0.7)
        {
            warn!(
                "Self-improvement warning before {}: potential {} pattern ({}%)",
                name,
                warning.error_type,
                (warning.likelihood * 100.0) as u32
            );
        }

        let args_str = inject_runtime_tool_defaults(&self.config, &name, &args_str);
        let (call_id, use_native_fc, fake_call) =
            self.build_tool_call_context(&name, &args_str, tool_call_id);

        if self
            .suppress_repeated_failed_tool_retry(
                &name,
                &args_str,
                &call_id,
                use_native_fc,
                start_time,
            )
            .await
        {
            self.emit_event(AgentEvent::ToolCompleted {
                name: name.clone(),
                success: false,
                duration_ms: start_time.elapsed().as_millis() as u64,
            });
            return Ok(());
        }

        if self
            .maybe_block_dependency_spiral(&name, &args_str, &call_id, use_native_fc, start_time)
            .await
        {
            self.emit_event(AgentEvent::ToolCompleted {
                name: name.clone(),
                success: false,
                duration_ms: start_time.elapsed().as_millis() as u64,
            });
            return Ok(());
        }

        if self
            .maybe_block_repeated_probe(&name, &args_str, &call_id, use_native_fc, start_time)
            .await
        {
            self.emit_event(AgentEvent::ToolCompleted {
                name: name.clone(),
                success: false,
                duration_ms: start_time.elapsed().as_millis() as u64,
            });
            return Ok(());
        }

        if let Some(error_msg) = self.current_task_tool_policy_violation(&name) {
            self.reject_tool_call_before_execution(
                &name,
                &args_str,
                &call_id,
                use_native_fc,
                start_time,
                "task_policy",
                &error_msg,
            )
            .await;
            self.emit_event(AgentEvent::ToolCompleted {
                name: name.clone(),
                success: false,
                duration_ms: start_time.elapsed().as_millis() as u64,
            });
            return Ok(());
        }

        if let Err(e) = self.safety.check_tool_call(&fake_call) {
            let error_msg = format!("Safety check failed: {}", e);
            let spinner = crate::ui::spinner::TerminalSpinner::start(&error_msg);
            spinner.stop_error(&error_msg);
            crate::output::safety_blocked(&error_msg);
            if let Some(ref logger) = self.audit_logger {
                logger.log_safety_block(&name, &error_msg);
            }
            self.push_tool_result_message(
                use_native_fc,
                &call_id,
                &name,
                &args_str,
                false,
                &error_msg,
            )
            .await;
            self.log_tool_call(&name, &args_str, &error_msg, false, start_time, false);
            self.remember_failed_tool(&name, &error_msg);
            let duration_ms = start_time.elapsed().as_millis() as u64;
            self.self_improvement.record_tool(
                &name,
                self.learning_context(),
                Outcome::Failure,
                duration_ms,
                Some(error_msg.clone()),
            );
            self.self_improvement.record_error(
                &error_msg,
                "safety",
                self.learning_context(),
                &name,
                None,
            );
            self.record_failed_tool_attempt(&name, &args_str, "safety", &error_msg);
            return Ok(());
        }

        let args = match self
            .parse_tool_args(&name, &args_str, &call_id, use_native_fc, start_time)
            .await
        {
            Some(args) => args,
            None => {
                self.emit_event(AgentEvent::ToolCompleted {
                    name: name.clone(),
                    success: false,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                });
                return Ok(());
            }
        };

        if !self
            .validate_tool_args(&name, &args_str, &args, &call_id, use_native_fc, start_time)
            .await
        {
            self.emit_event(AgentEvent::ToolCompleted {
                name: name.clone(),
                success: false,
                duration_ms: start_time.elapsed().as_millis() as u64,
            });
            return Ok(());
        }

        if self
            .maybe_block_redundant_reread(
                &name,
                &args_str,
                &args,
                &call_id,
                use_native_fc,
                start_time,
            )
            .await
        {
            self.emit_event(AgentEvent::ToolCompleted {
                name: name.clone(),
                success: false,
                duration_ms: start_time.elapsed().as_millis() as u64,
            });
            return Ok(());
        }

        if !self
            .confirm_tool_execution(&name, &args_str, &call_id, use_native_fc)
            .await?
        {
            return Ok(());
        }

        // Fire PreToolUse hooks (may skip execution)
        let pre_ctx = HookContext::pre_tool(&name, &args_str);
        if let HookAction::Skip { reason } = self.hook_registry.fire(&pre_ctx).await {
            let skip_msg = format!("Tool skipped by PreToolUse hook: {}", reason);
            info!("{}", skip_msg);
            self.push_tool_result_message(
                use_native_fc,
                &call_id,
                &name,
                &args_str,
                false,
                &skip_msg,
            )
            .await;
            return Ok(());
        }

        self.emit_event(AgentEvent::ToolStarted { name: name.clone() });

        let activity = crate::output::tool_activity_message(&name, &args);
        let spinner = crate::ui::spinner::TerminalSpinner::start(&activity);
        let (success, result, summary) = self
            .execute_single_tool(&name, &args_str, &args, start_time)
            .await?;

        let duration_ms = start_time.elapsed().as_millis() as u64;
        self.emit_event(AgentEvent::ToolCompleted {
            name: name.clone(),
            success,
            duration_ms,
        });

        if success {
            spinner.stop_success(&summary);
        } else {
            spinner.stop_error(&summary);
        }

        // Store for progressive disclosure via /last
        {
            let exit_code = serde_json::from_str::<serde_json::Value>(&result)
                .ok()
                .and_then(|v| v.get("exit_code").and_then(|c| c.as_i64()))
                .map(|c| c as i32);
            self.store_last_tool_output(crate::agent::last_tool::LastToolOutput {
                tool_name: name.clone(),
                summary: summary.clone(),
                full_output: result.clone(),
                success,
                exit_code,
                duration_ms,
            });
        }

        let tool_outcome = if success {
            Outcome::Success
        } else {
            Outcome::Failure
        };
        let tool_error = (!success).then(|| result.clone());
        self.self_improvement.record_tool(
            &name,
            self.learning_context(),
            tool_outcome,
            duration_ms,
            tool_error.clone(),
        );
        if let Some(error_text) = tool_error {
            self.self_improvement.record_error(
                &error_text,
                Self::classify_error_type(&error_text),
                self.learning_context(),
                &name,
                None,
            );
        }
        if success {
            self.clear_failed_tool_attempts();
        } else {
            self.record_failed_tool_attempt(&name, &args_str, "execution", &result);
        }

        // Dependency-firewall accounting (loop 9).
        if matches!(name.as_str(), "shell_exec" | "pty_shell") {
            if let Some(cmd) = args.get("command").and_then(|c| c.as_str()) {
                self.note_shell_outcome(cmd, success);
            }
        }

        // Best-snapshot capture on green verification.
        self.note_green_verification(&name, &args_str, success);

        // Stagnation accounting (loop 13d).
        self.note_workspace_state(&name, &args_str, success)?;

        self.track_task_state_after_tool(&name, &args, &result, success)
            .await;

        // Track file operations for context management
        if success {
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                let path_str = path.to_string();
                match name.as_str() {
                    "file_read" => {
                        self.last_read_file = Some(path_str.clone());
                        if self.file_tracker.context_files.len() < 500
                            && !self.file_tracker.context_files.contains(&path_str)
                        {
                            self.file_tracker.context_files.push(path_str.clone());
                        }
                        if let Some(content) = serde_json::from_str::<serde_json::Value>(&result)
                            .ok()
                            .and_then(|v| {
                                v.get("content").and_then(|c| c.as_str()).map(String::from)
                            })
                        {
                            self.track_file_read_in_context_map(&path_str, &content)
                                .await;
                        }
                    }
                    "file_delete" => {
                        self.file_tracker.remove_deleted(&path_str);
                    }
                    "file_write" | "file_edit" => {
                        self.file_tracker.mark_stale(&path_str);
                    }
                    _ => {}
                }
            }
        }

        self.push_tool_result_message(use_native_fc, &call_id, &name, &args_str, success, &result)
            .await;
        if !success {
            self.remember_failed_tool(&name, &result);
        }

        // Reset no-action counter - the model attempted to use a tool
        // (even if it failed, this counts as taking action)
        self.reset_no_action_prompt_state();

        // Add post-error guidance for failed tools to help model recover
        if !success {
            let recovery_hint = self.build_error_recovery_hint(&name, &result);
            self.messages.push(Message::user(recovery_hint));
        }

        // Fire PostToolUse hooks (e.g., auto-format, lint, auto-commit)
        let post_ctx = HookContext::post_tool(&name, &args_str, success, &result);
        self.hook_registry.fire(&post_ctx).await;

        // Audit: log tool execution
        if let Some(ref logger) = self.audit_logger {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            args_str.hash(&mut hasher);
            let args_hash = format!("{:x}", hasher.finish());
            logger.log_tool_execution(&name, &args_hash, success, duration_ms, None);
        }

        Ok(())
    }

    /// Execute a context management tool (operates on agent state, not filesystem).
    async fn execute_context_tool_async(
        &mut self,
        name: &str,
        args: &serde_json::Value,
    ) -> serde_json::Value {
        use crate::tools::context::*;

        match name {
            CONTEXT_BULK_READ => {
                let pattern = args
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .unwrap_or("src/**/*.rs");
                let max_files =
                    args.get("max_files").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

                // Collect matching files from context map.
                let root = super::current_project_root();
                let mut paths: Vec<std::path::PathBuf> = Vec::new();
                let glob_pattern = root.join(pattern).to_string_lossy().to_string();
                if let Ok(entries) = glob::glob(&glob_pattern) {
                    for entry in entries.flatten() {
                        if let Ok(rel) = entry.strip_prefix(&root) {
                            paths.push(rel.to_path_buf());
                        }
                        if paths.len() >= max_files {
                            break;
                        }
                    }
                }

                let total_files = paths.len();
                let (loaded, skipped, tokens) = self.parallel_bulk_read(paths).await;

                serde_json::json!({
                    "matched_files": total_files,
                    "loaded": loaded,
                    "skipped": skipped,
                    "tokens_added": tokens,
                    "context_usage_pct": format!("{:.1}%", self.context_map.usage_fraction() * 100.0),
                })
            }
            CONTEXT_SUMMARY => {
                let summary = self.generate_structured_summary();
                serde_json::json!({
                    "summary": summary,
                    "total_tokens": self.context_map.total_tokens(),
                    "budget": self.context_map.budget(),
                })
            }
            CONTEXT_STATUS => {
                let stats = self.context_map.stats();
                serde_json::json!({
                    "total_tokens": stats.total_tokens,
                    "budget": stats.budget,
                    "usage_pct": format!("{:.1}%", (stats.total_tokens as f64 / stats.budget.max(1) as f64) * 100.0),
                    "remaining": self.context_map.remaining(),
                    "l1_tree": { "count": stats.l1_count, "tokens": stats.l1_tokens },
                    "l2_skeleton": { "count": stats.l2_count, "tokens": stats.l2_tokens },
                    "l3_full": { "count": stats.l3_count, "tokens": stats.l3_tokens },
                    "compression_headroom": self.context_map.compression_headroom(),
                    "thinking_reserve": self.context_map.thinking_reserve(),
                })
            }
            CONTEXT_FOCUS => {
                let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                let max_files =
                    args.get("max_files").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

                let to_promote = self.context_map.focus_on_query(query, max_files).await;

                // Actually load the files that need promoting.
                let root = super::current_project_root();
                let mut loaded = Vec::new();
                for path in &to_promote {
                    let full_path = root.join(path);
                    if let Ok(content) = tokio::fs::read_to_string(&full_path).await {
                        self.context_map.load_full(path, content);
                        loaded.push(path.to_string_lossy().to_string());
                    }
                }

                let stats = self.context_map.stats();
                serde_json::json!({
                    "promoted": loaded,
                    "query": query,
                    "total_tokens_after": stats.total_tokens,
                    "budget": stats.budget,
                })
            }
            CONTEXT_EVICT => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let freed = self.context_map.evict_to_tree(std::path::Path::new(path));
                serde_json::json!({
                    "evicted": path,
                    "tokens_freed": freed,
                    "remaining": self.context_map.remaining(),
                })
            }
            CONTEXT_RECOMMEND => {
                let task = args.get("task").and_then(|v| v.as_str()).unwrap_or("");
                let rec = self.context_map.recommend_context(task).await;
                serde_json::json!({
                    "modality": rec.modality_description,
                    "potential_savings": rec.potential_token_savings,
                    "promote": rec.promote.iter().map(|s| serde_json::json!({
                        "path": s.path.to_string_lossy(),
                        "from": format!("{:?}", s.current_level),
                        "to": format!("{:?}", s.suggested_level),
                        "reason": s.reason,
                        "estimated_tokens": s.estimated_tokens,
                    })).collect::<Vec<_>>(),
                    "evict": rec.evict.iter().map(|s| serde_json::json!({
                        "path": s.path.to_string_lossy(),
                        "from": format!("{:?}", s.current_level),
                        "to": format!("{:?}", s.suggested_level),
                        "reason": s.reason,
                    })).collect::<Vec<_>>(),
                })
            }
            CONTEXT_LOAD_SKELETON => {
                let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let path = std::path::Path::new(path_str);
                let root = super::current_project_root();
                let full_path = root.join(path);

                match tokio::fs::read_to_string(&full_path).await {
                    Ok(content) => {
                        let skeleton = super::context_map::extract_rust_skeleton(path, &content);
                        let rendered = skeleton.render();
                        let token_count = skeleton.token_count;
                        self.context_map.load_skeleton(path, skeleton);
                        serde_json::json!({
                            "path": path_str,
                            "skeleton": rendered,
                            "token_count": token_count,
                            "level": "L2",
                        })
                    }
                    Err(e) => {
                        serde_json::json!({
                            "error": format!("Failed to read {}: {}", path_str, e),
                        })
                    }
                }
            }
            _ => serde_json::json!({ "error": format!("Unknown context tool: {}", name) }),
        }
    }

    /// Execute tool_search - search for deferred tools and activate them.
    /// This allows the LLM to discover tools on demand, reducing context window usage.
    async fn execute_tool_search(&mut self, args: &serde_json::Value) -> serde_json::Value {
        use crate::tools::tool_search::ToolSearchResult;

        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");

        if query.is_empty() {
            return serde_json::json!({
                "error": "query parameter is required",
                "found_tools": [],
                "count": 0,
            });
        }

        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(5)
            .clamp(1, 20);

        // Search for tools in the registry
        let results: Vec<ToolSearchResult> = self.tools.search(query, limit);

        // Activate the found tools (make them available for use)
        let mut activated = Vec::new();
        for result in &results {
            if !result.is_critical && !self.tools.is_activated(&result.name) {
                self.tools.activate(&result.name);
                activated.push(result.name.clone());
            }
        }

        // Build response
        let found_tools: Vec<serde_json::Value> = results
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "name": r.name,
                    "description": r.description,
                    "schema": r.schema,
                    "is_critical": r.is_critical,
                    "category": r.category,
                })
            })
            .collect();

        let total_tools = self.tools.total_count();
        let activated_tools = self.tools.activated_count();

        serde_json::json!({
            "found_tools": found_tools,
            "count": found_tools.len(),
            "query": query,
            "newly_activated": activated,
            "total_tools_available": total_tools,
            "activated_tools_count": activated_tools,
            "note": if activated.is_empty() {
                "These tools are available for use in this session."
            } else {
                "These tools are now available for use in this session."
            },
        })
    }

    pub(super) fn build_tool_call_context(
        &self,
        name: &str,
        args_str: &str,
        tool_call_id: Option<String>,
    ) -> (String, bool, crate::api::types::ToolCall) {
        let use_native_fc = self.config.agent.native_function_calling && tool_call_id.is_some();
        let call_id = tool_call_id.unwrap_or_else(|| format!("call_{}", uuid::Uuid::new_v4()));
        let fake_call = crate::api::types::ToolCall {
            id: call_id.clone(),
            call_type: "function".to_string(),
            function: crate::api::types::ToolFunction {
                name: name.to_string(),
                arguments: args_str.to_string(),
            },
        };
        (call_id, use_native_fc, fake_call)
    }

    /// Push a `<tool_result><skipped>...</skipped></tool_result>` (or native
    /// tool-message equivalent) recording that a tool call was denied/skipped
    /// without being executed.
    fn push_tool_skip_message(&mut self, call_id: &str, use_native_fc: bool, msg: &str) {
        if use_native_fc {
            self.messages.push(Message::tool(
                serde_json::json!({"skipped": msg}).to_string(),
                call_id,
            ));
        } else {
            self.messages.push(Message::user(format!(
                "<tool_result><skipped>{}</skipped></tool_result>",
                msg
            )));
        }
    }

    async fn confirm_tool_execution(
        &mut self,
        name: &str,
        args_str: &str,
        call_id: &str,
        use_native_fc: bool,
    ) -> Result<bool> {
        // In Yolo/Daemon mode `needs_confirmation()` below always says "no
        // need to ask" -- but the YoloManager still enforces a hard floor:
        // forbidden operations, protected paths, dangerous container mounts,
        // and (unless explicitly allowed in config) destructive shell
        // commands / unconfirmed git pushes. This is the only place those
        // checks run, so it must not be skipped.
        if matches!(
            self.config.execution_mode,
            crate::config::ExecutionMode::Yolo | crate::config::ExecutionMode::Daemon
        ) {
            let args_value: serde_json::Value =
                serde_json::from_str(args_str).unwrap_or(serde_json::Value::Null);
            let decision = self.yolo_manager.should_auto_approve(name, &args_value);
            use crate::safety::yolo::YoloDecision;
            match decision {
                YoloDecision::AutoApprove => {
                    self.yolo_manager.record_operation(
                        name,
                        &args_value,
                        true,
                        crate::safety::yolo::AuditResult::Success,
                        0,
                    );
                }
                YoloDecision::Block(reason) => {
                    self.yolo_manager.record_operation(
                        name,
                        &args_value,
                        false,
                        crate::safety::yolo::AuditResult::Blocked(reason.clone()),
                        0,
                    );
                    self.push_tool_skip_message(
                        call_id,
                        use_native_fc,
                        &format!("Blocked by YOLO safety gate: {}", reason),
                    );
                    return Ok(false);
                }
                YoloDecision::RequireConfirmation(reason) => {
                    // No operator to ask in a headless/daemon run -- fail
                    // closed rather than silently allowing or hanging.
                    if !self.is_interactive() && !self.has_tui_renderer() {
                        self.yolo_manager.record_operation(
                            name,
                            &args_value,
                            false,
                            crate::safety::yolo::AuditResult::Blocked(reason.clone()),
                            0,
                        );
                        self.push_tool_skip_message(
                            call_id,
                            use_native_fc,
                            &format!(
                                "Denied (unattended session, no operator to confirm): {}",
                                reason
                            ),
                        );
                        return Ok(false);
                    }
                    // An operator is present (CLI or TUI): fall through to
                    // the normal interactive prompt below instead of the
                    // usual YOLO auto-approve.
                    return self
                        .prompt_tool_confirmation(name, args_str, call_id, use_native_fc)
                        .await;
                }
            }
        }

        // Normal mode decides via the tool-metadata classification (P1-5):
        // read-only/Low-risk tools (`lsp_diagnostics`, `process_list`,
        // `ask_user`, ...) no longer prompt — only Medium/High-risk tools do.
        // Session permission grants ("always allow") and the operator's
        // `safety.require_confirmation` list keep their precedence. The other
        // modes keep the legacy `needs_confirmation()` rules.
        let confirmation_needed = if matches!(
            self.config.execution_mode,
            crate::config::ExecutionMode::Normal
        ) {
            crate::safety::normal_mode_needs_confirmation(
                name,
                &self.config.safety.require_confirmation,
                &self.permission_store,
            )
        } else {
            self.needs_confirmation(name)
        };
        if !confirmation_needed {
            return Ok(true);
        }

        self.prompt_tool_confirmation(name, args_str, call_id, use_native_fc)
            .await
    }

    /// Interactive (CLI or TUI) yes/no confirmation prompt for a single tool
    /// call. Assumes the caller has already decided confirmation is required.
    async fn prompt_tool_confirmation(
        &mut self,
        name: &str,
        args_str: &str,
        call_id: &str,
        use_native_fc: bool,
    ) -> Result<bool> {
        let args_preview: String = args_str
            .chars()
            .take(TOOL_CONFIRM_ARGS_PREVIEW_CHARS)
            .collect();
        let args_display = if args_str.chars().count() > TOOL_CONFIRM_ARGS_PREVIEW_CHARS {
            format!("{}...", args_preview)
        } else {
            args_preview
        };

        // When TUI is active, route the confirmation through the TUI's own
        // permission modal instead of writing to stdout/stdin (which the TUI
        // owns) or auto-approving.
        if self.has_tui_renderer() {
            use super::tui_events::AgentEvent;
            self.emit_event(AgentEvent::PermissionRequested {
                tool_name: name.to_string(),
                reason: format!("Args: {}", args_display),
            });
            let approved = self.await_tui_permission_response().await;
            if !approved {
                let denial = "Tool execution denied via TUI permission prompt";
                self.record_failed_tool_attempt(name, args_str, "operator_denied", denial);
                self.push_tool_skip_message(call_id, use_native_fc, denial);
            }
            return Ok(approved);
        }

        if !self.is_interactive() {
            return Err(anyhow::anyhow!(
                "Tool '{}' requires confirmation but cannot prompt in headless mode. \
                 Re-run with --yolo to auto-approve all tool calls, \
                 or use interactive/TUI mode for manual confirmation.",
                name
            ));
        }

        // Leading newline separates the block from any unterminated streaming
        // output; the prompt goes through the locked cli_prompt! so it can never
        // interleave with concurrent managed output.
        cli_println!(
            "\n{} Tool: {} Args: {}",
            "⚠️".bright_yellow(),
            name.bright_cyan(),
            args_display.bright_white()
        );
        cli_prompt!("\x1b[0m\x1b[1m\x1b[97mExecute? [y = once / a = always allow this tool (session) / N = skip / type \"yolo\" to disable confirmations]: \x1b[0m");

        let response =
            super::execution::read_line_pausing_esc(&self.esc_paused, &self.esc_pause_ack).await;
        if let Ok(response) = response {
            match parse_confirm_response(&response) {
                ConfirmDecision::ExecuteOnce => return Ok(true),
                ConfirmDecision::AlwaysAllow => {
                    // Session-scoped grant: `needs_confirmation`/`normal_mode_
                    // needs_confirmation` consult the permission store first,
                    // so future calls of this tool skip the prompt.
                    self.permission_store
                        .add(crate::safety::permissions::PermissionGrant::session(name));
                    cli_println!(
                        "{} '{}' allowed for the rest of this session",
                        "✓".bright_green(),
                        name.bright_cyan()
                    );
                    return Ok(true);
                }
                ConfirmDecision::EnableYolo => {
                    self.set_execution_mode(crate::config::ExecutionMode::Yolo);
                    cli_println!(
                        "{} Confirmations disabled for the rest of this session (YOLO)",
                        "⚡".bright_yellow()
                    );
                    return Ok(true);
                }
                ConfirmDecision::Skip => {}
            }
        }

        let skip_msg = "Tool execution skipped by user";
        self.record_failed_tool_attempt(name, args_str, "operator_denied", skip_msg);
        cli_println!("{} {}", "⏭️".bright_yellow(), skip_msg);
        self.push_tool_skip_message(call_id, use_native_fc, skip_msg);
        Ok(false)
    }

    pub(super) async fn parse_tool_args(
        &mut self,
        name: &str,
        args_str: &str,
        call_id: &str,
        use_native_fc: bool,
        start_time: std::time::Instant,
    ) -> Option<Value> {
        match serde_json::from_str(args_str) {
            Ok(args) => {
                debug!("Tool arguments: {}", args);
                Some(args)
            }
            Err(e) => {
                let err = format!("Invalid JSON arguments: {}", e);
                cli_println!("{} {}", "✗".bright_red(), err);
                self.push_tool_result_message(use_native_fc, call_id, name, args_str, false, &err)
                    .await;
                self.log_tool_call(name, args_str, &err, false, start_time, false);
                self.log_tool_validation_failure_event(
                    name,
                    args_str,
                    &err,
                    call_id,
                    use_native_fc,
                );
                self.remember_failed_tool(name, &err);
                let duration_ms = start_time.elapsed().as_millis() as u64;
                self.self_improvement.record_tool(
                    name,
                    self.learning_context(),
                    Outcome::Failure,
                    duration_ms,
                    Some(err.clone()),
                );
                self.self_improvement.record_error(
                    &err,
                    "parsing",
                    self.learning_context(),
                    name,
                    None,
                );
                self.record_failed_tool_attempt(name, args_str, "parsing", &err);
                None
            }
        }
    }

    pub(super) async fn validate_tool_args(
        &mut self,
        name: &str,
        args_str: &str,
        args: &Value,
        call_id: &str,
        use_native_fc: bool,
        start_time: std::time::Instant,
    ) -> bool {
        let Some(tool) = self.tools.get(name) else {
            return true;
        };

        match crate::tools::validate_tool_arguments_schema(name, &tool.schema(), args) {
            Ok(()) => true,
            Err(e) => {
                let err = e.to_string();
                cli_println!("{} {}", "✗".bright_red(), err);
                self.push_tool_result_message(use_native_fc, call_id, name, args_str, false, &err)
                    .await;
                self.log_tool_call(name, args_str, &err, false, start_time, false);
                self.log_tool_validation_failure_event(
                    name,
                    args_str,
                    &err,
                    call_id,
                    use_native_fc,
                );
                self.remember_failed_tool(name, &err);
                let duration_ms = start_time.elapsed().as_millis() as u64;
                self.self_improvement.record_tool(
                    name,
                    self.learning_context(),
                    Outcome::Failure,
                    duration_ms,
                    Some(err.clone()),
                );
                self.self_improvement.record_error(
                    &err,
                    "validation",
                    self.learning_context(),
                    name,
                    None,
                );
                self.record_failed_tool_attempt(name, args_str, "validation", &err);
                false
            }
        }
    }

    /// Snapshot every file a multi-file mutating tool is about to touch so
    /// `/undo` can restore ALL of them, not none. Creates one `MultiFileEdit`
    /// checkpoint; if no target could be read the checkpoint is left empty and
    /// `/undo` honestly reports "no files to restore" instead of silently
    /// reverting an older, unrelated checkpoint while claiming success.
    ///
    /// Free function (not a method) so it can borrow `self.edit_history`
    /// disjointly from the immutable `self.tools` borrow held at the call site.
    pub(super) async fn snapshot_files_for_undo(
        history: &mut crate::session::edit_history::EditHistory,
        mut paths: Vec<std::path::PathBuf>,
        tool: &str,
    ) {
        paths.sort();
        paths.dedup();
        if paths.is_empty() {
            return;
        }
        use crate::session::edit_history::{EditAction, FileSnapshot};
        let action = EditAction::MultiFileEdit {
            paths: paths.clone(),
            tool: tool.to_string(),
        };
        history.create_checkpoint(action);
        for path in &paths {
            if let Ok(content) = tokio::fs::read_to_string(path).await {
                history.add_file_to_current(FileSnapshot::new(path.clone(), content));
            }
        }
    }

    pub(super) async fn execute_single_tool(
        &mut self,
        name: &str,
        args_str: &str,
        args: &Value,
        start_time: std::time::Instant,
    ) -> Result<(bool, String, String)> {
        // Track every dispatched tool call for FailureMode classification.
        self.note_total_tool_call();

        // Emit a `ToolCallStarted` progress event before dispatch. The matching
        // `ToolCallCompleted` event is emitted at the end via the inner helper
        // so we don't have to thread it through every early-return branch.
        self.emit_progress(super::progress::ProgressEvent::ToolCallStarted {
            tool: name.to_string(),
            args_short: super::progress::short_args_for(name, args),
        });

        let result = self
            .execute_single_tool_inner(name, args_str, args, start_time)
            .await;
        let elapsed_ms = start_time.elapsed().as_millis() as u64;
        let ok = matches!(&result, Ok((true, _, _)));
        self.emit_progress(super::progress::ProgressEvent::ToolCallCompleted {
            tool: name.to_string(),
            ok,
            elapsed_ms,
        });
        result
    }

    /// Inner body of [`execute_single_tool`] — kept as a separate method so the
    /// outer wrapper can emit `ToolCallStarted` / `ToolCallCompleted` progress
    /// events around it without threading them through every early return.
    async fn execute_single_tool_inner(
        &mut self,
        name: &str,
        args_str: &str,
        args: &Value,
        start_time: std::time::Instant,
    ) -> Result<(bool, String, String)> {
        // Intercept context management tools — they operate on agent state,
        // not the filesystem, so they bypass the normal tool registry.
        if crate::tools::context::is_context_tool(name) {
            let result = self.execute_context_tool_async(name, args).await;
            let elapsed = start_time.elapsed().as_millis() as u64;
            let result_str = serde_json::to_string(&result)?;
            let summary =
                crate::output::semantic_summary(name, args, Some(&result_str), true, elapsed);
            self.log_tool_call(name, args_str, &result_str, true, start_time, true);
            return Ok((true, result_str, summary));
        }

        // Intercept tool_search — it activates deferred tools and returns their schemas
        if name == "tool_search" {
            let result = self.execute_tool_search(args).await;
            let elapsed = start_time.elapsed().as_millis() as u64;
            let result_str = serde_json::to_string(&result)?;
            let summary =
                crate::output::semantic_summary(name, args, Some(&result_str), true, elapsed);
            self.log_tool_call(name, args_str, &result_str, true, start_time, true);
            return Ok((true, result_str, summary));
        }

        let Some(tool) = self.tools.get(name) else {
            let err = format!("Unknown tool: {}", name);
            self.log_tool_call(name, args_str, &err, false, start_time, false);
            return Ok((false, err.clone(), err));
        };

        // Check ToolCache for cacheable (read-only) tools
        let is_cacheable = crate::session::cache::is_cacheable(name);
        if is_cacheable {
            if let Some(cached_value) = self.cache_manager.tool_cache.get(name, args).await {
                let elapsed = start_time.elapsed().as_millis() as u64;
                let result_str = serde_json::to_string(&cached_value)?;
                let summary =
                    crate::output::semantic_summary(name, args, Some(&result_str), true, elapsed);
                self.log_tool_call(name, args_str, &result_str, true, start_time, true);
                debug!("Cache hit for tool '{}' ({}ms)", name, elapsed);
                return Ok((true, result_str, summary));
            }
        }

        // Invalidate cache entries when a mutating tool targets a specific path
        if crate::session::cache::invalidates_cache(name) {
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                self.cache_manager.invalidate_path(path).await;
            }
            // Mutations that can't be reduced to one `path` arg — shells run
            // arbitrary commands, file_multi_edit carries an `edits` array,
            // patch_apply a diff — can affect anything, and cached
            // git_status/git_diff/grep results don't contain the edited path
            // in their key anyway. Clear all read caches so the agent never
            // sees pre-edit output and concludes its edit vanished.
            if matches!(
                name,
                "shell_exec"
                    | "pty_shell"
                    | "git_commit"
                    | "git_checkout"
                    | "file_multi_edit"
                    | "patch_apply"
            ) {
                self.cache_manager.tool_cache.clear().await;
            }
        }

        // Snapshot file before edit/write for undo support + diff display.
        let pre_edit_content: Option<(String, String)> =
            if matches!(name, "file_edit" | "file_write" | "file_delete") {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    if let Ok(content) = tokio::fs::read_to_string(path).await {
                        use crate::session::edit_history::{EditAction, FileSnapshot};
                        let snapshot =
                            FileSnapshot::new(std::path::PathBuf::from(path), content.clone());
                        let action = EditAction::FileEdit {
                            path: std::path::PathBuf::from(path),
                            tool: name.to_string(),
                        };
                        self.edit_history.create_checkpoint(action);
                        self.edit_history.add_file_to_current(snapshot);
                        Some((path.to_string(), content))
                    } else {
                        // New file (file_write to nonexistent path)
                        Some((path.to_string(), String::new()))
                    }
                } else {
                    None
                }
            } else if name == "file_multi_edit" {
                // Snapshot EVERY targeted file so `/undo` restores the whole
                // batch — previously no checkpoint was captured at all and
                // `/undo` silently reverted an older, unrelated checkpoint.
                let paths: Vec<std::path::PathBuf> = args
                    .get("edits")
                    .and_then(|v| v.as_array())
                    .map(|edits| {
                        edits
                            .iter()
                            .filter_map(|e| e.get("path").and_then(|p| p.as_str()))
                            .map(std::path::PathBuf::from)
                            .collect()
                    })
                    .unwrap_or_default();
                Self::snapshot_files_for_undo(&mut self.edit_history, paths, name).await;
                None
            } else if name == "patch_apply" {
                let paths = args
                    .get("diff")
                    .and_then(|v| v.as_str())
                    .map(patch_target_paths)
                    .unwrap_or_default();
                Self::snapshot_files_for_undo(&mut self.edit_history, paths, name).await;
                None
            } else {
                None
            };

        // Acquire concurrency governor permit before executing the tool.
        // The permit is held for the duration of execution and released on drop.
        let _tool_permit = self
            .governor
            .acquire_tool()
            .await
            .map_err(|e| anyhow::anyhow!("concurrency governor error: {}", e))?;

        // Track bash/shell commands for the sticky status bar.
        // The guard decrements on drop regardless of how execution exits.
        let is_bash = matches!(name, "shell_exec" | "pty_shell");
        let _bash_guard: Option<crate::ui::sticky_bar::BashGuard> = if is_bash {
            Some(crate::ui::sticky_bar::BashGuard::new())
        } else {
            None
        };

        let timeout_secs = self.config.agent.step_timeout_secs.max(1);

        // For tools that spawn an OS subprocess (shell_exec, pty_shell), emit
        // structured progress events around the spawn so live observers
        // (StderrProgressEmitter, future Prometheus exporter) can track
        // subprocess lifecycles independently of the in-process tool wrapper.
        let spawns_subprocess = is_bash;
        let subprocess_start = std::time::Instant::now();
        if spawns_subprocess {
            self.emit_progress(super::progress::ProgressEvent::SubprocessStarted {
                name: name.to_string(),
            });
        }

        let execution = run_tool_bounded(
            crate::observability::telemetry::track_tool_execution(name, || {
                tool.execute(args.clone())
            }),
            std::time::Duration::from_secs(timeout_secs),
            self.cancel_token(),
        )
        .await;

        if spawns_subprocess {
            // Best-effort exit code: tool wrappers (e.g. shell_exec) surface it
            // in the JSON result; the agent layer can't read it back here, so we
            // report success/failure as 0/-1 and the elapsed wall time.
            let exit = match &execution {
                Ok(Ok(_)) => 0,
                Ok(Err(_)) => -1,
                Err(_) => -2, // tokio timeout
            };
            self.emit_progress(super::progress::ProgressEvent::SubprocessCompleted {
                name: name.to_string(),
                exit,
                elapsed_ms: subprocess_start.elapsed().as_millis() as u64,
            });
        }

        match execution {
            Ok(Ok(result)) => {
                let elapsed = start_time.elapsed().as_millis() as u64;
                let result_str = serde_json::to_string(&result)?;
                let tool_success = tool_result_value_indicates_success(&result);
                let summary = crate::output::semantic_summary(
                    name,
                    args,
                    Some(&result_str),
                    tool_success,
                    elapsed,
                );
                self.log_tool_call(name, args_str, &result_str, tool_success, start_time, true);

                // Store successful cacheable results in ToolCache
                if is_cacheable && tool_success {
                    self.cache_manager
                        .tool_cache
                        .set(name, args, result.clone())
                        .await;
                }

                // Cache tool results in LocalFirstCoordinator
                if tool_success {
                    let cache_key = crate::session::cache::ToolCache::cache_key(name, args);
                    self.cache_manager.local_first.cache_response(
                        &cache_key,
                        result_str.clone(),
                        result_str.len(),
                    );
                }

                // Display color-coded diff for file mutations
                if let Some((ref path, ref old_content)) = pre_edit_content {
                    if tool_success && matches!(name, "file_edit" | "file_write") {
                        self.has_written_any_file = true;
                        self.terminal_guard_hits = 0;
                        if let Ok(new_content) = tokio::fs::read_to_string(path).await {
                            crate::output::display_file_diff(path, old_content, &new_content);
                        }
                    }
                }

                // Track mutating tool calls for FailureMode classification.
                // For `shell_exec`, only count as mutating when the command is
                // NOT observational (e.g. `rm`, `mv`, `git add`, `cargo fmt`,
                // `sed -i`, redirects).  Observational shell calls like
                // `cargo check` / `git status` / `ls` should NOT bump the
                // mutating counter.
                if tool_call_is_mutating(name, args) && tool_success {
                    self.note_mutating_tool_call();
                }

                self.note_verification_outcome(name, args_str, tool_success, &result_str);

                // Record successful tool usage for learning
                self.self_improvement.record_tool(
                    name,
                    self.learning_context(),
                    if tool_success {
                        Outcome::Success
                    } else {
                        Outcome::Failure
                    },
                    elapsed,
                    (!tool_success).then(|| result_str.clone()),
                );

                let verification_result = if tool_success {
                    self.maybe_verify_file_change(name, args).await
                } else {
                    None
                };
                let visual_verification_result = self.maybe_verify_visual_change(name, args).await;
                let enhanced_result = self.maybe_enhance_tool_result(name, &result_str);
                let mut final_result = enhanced_result;
                if let Some(ver_msg) = verification_result {
                    final_result.push_str(&ver_msg);
                }
                // Track visual verification details for potential error reporting
                let mut hard_failure_details: Option<(String, String, String)> = None;
                if let Some(ref vvr) = visual_verification_result {
                    if !vvr.message.is_empty() {
                        final_result.push_str(&vvr.message);
                    }
                    if let Some(ref assertion) = vvr.assertion {
                        if let Some(ref mut checkpoint) = self.current_checkpoint {
                            // On hard failure, set as pending assertion to gate progression
                            if vvr.hard_failure {
                                checkpoint.set_pending_visual_assertion(assertion.clone());
                            } else {
                                checkpoint.log_visual_assertion(assertion.clone());
                            }
                        }
                        // Capture details for error message if this is a hard failure
                        if vvr.hard_failure {
                            let exp = assertion
                                .expected
                                .clone()
                                .unwrap_or_else(|| "Expected UI state".to_string());
                            let obs = assertion
                                .observed
                                .clone()
                                .unwrap_or_else(|| "Actual UI state did not match".to_string());
                            // Extract issues from the message if present
                            let iss = if vvr.message.contains("issues:") {
                                vvr.message
                                    .split("issues:")
                                    .nth(1)
                                    .map(|s| s.trim().to_string())
                                    .unwrap_or_else(|| "No specific issues listed".to_string())
                            } else {
                                "No specific issues listed".to_string()
                            };
                            hard_failure_details = Some((exp, obs, iss));
                        }
                    }
                }
                if let Some((expected, actual, issues)) = hard_failure_details {
                    // Return an error to trigger error recovery flow with rich details
                    return Err(crate::errors::AgentError::VisualAssertionFailed {
                        description: format!("Visual verification failed after {}: {}", name, issues),
                        expected,
                        actual,
                        recovery_hint: format!(
                            "The {} action did not produce the expected visual result. \
                             Retry the action with different parameters or try a different approach.",
                            name
                        ),
                    }.into());
                }
                Ok((tool_success, final_result, summary))
            }
            Ok(Err(e)) => {
                let elapsed = start_time.elapsed().as_millis() as u64;
                let summary = crate::output::semantic_summary(
                    name,
                    args,
                    Some(&e.to_string()),
                    false,
                    elapsed,
                );
                self.log_tool_call(name, args_str, &e.to_string(), false, start_time, false);
                self.cognitive_state
                    .episodic_memory
                    .what_failed(name, &e.to_string());

                // Record failed tool usage for learning
                self.self_improvement.record_tool(
                    name,
                    self.learning_context(),
                    Outcome::Failure,
                    elapsed,
                    Some(e.to_string()),
                );

                Ok((false, e.to_string(), summary))
            }
            Err(ToolHalt::TimedOut) => {
                let elapsed = start_time.elapsed().as_millis() as u64;
                let err = format!("Tool '{}' timed out after {}s", name, timeout_secs);
                let summary =
                    crate::output::semantic_summary(name, args, Some(&err), false, elapsed);
                self.log_tool_call(name, args_str, &err, false, start_time, false);
                self.cognitive_state.episodic_memory.what_failed(name, &err);
                self.self_improvement.record_tool(
                    name,
                    self.learning_context(),
                    Outcome::Failure,
                    elapsed,
                    Some(err.clone()),
                );
                Ok((false, err, summary))
            }
            Err(ToolHalt::Cancelled) => {
                let elapsed = start_time.elapsed().as_millis() as u64;
                let err = format!("Tool '{}' cancelled", name);
                let summary =
                    crate::output::semantic_summary(name, args, Some(&err), false, elapsed);
                self.log_tool_call(name, args_str, &err, false, start_time, false);
                Ok((false, err, summary))
            }
        }
    }

    pub(super) async fn push_tool_result_message(
        &mut self,
        use_native_fc: bool,
        call_id: &str,
        tool_name: &str,
        args_str: &str,
        success: bool,
        result: &str,
    ) {
        // Detect base64_png in successful tool results and promote to multimodal
        if success {
            if let Some(base64_png) = super::execution::try_extract_base64_png(result) {
                let summary = super::execution::build_image_result_summary(result);
                let content =
                    crate::api::types::MessageContent::from_text(&summary).with_image(&base64_png);
                if use_native_fc {
                    self.messages.push(crate::api::types::Message {
                        role: "tool".to_string(),
                        content,
                        reasoning_content: None,
                        tool_calls: None,
                        tool_call_id: Some(call_id.to_string()),
                        name: None,
                    });
                } else {
                    self.messages.push(Message::user_multimodal(content));
                }
                return;
            }
        }

        // Budget check: if the result exceeds the per-result token budget,
        // spill the raw data to disk and store a structured summary + reference.
        // Applies to BOTH success AND error results — an oversized error was
        // previously stored verbatim, which could blow the context-token budget
        // (an OOM surface). summarize_and_spill keeps head+tail, so trailing
        // failure markers still survive.
        let result_to_store = {
            let estimated_tokens = crate::token_count::estimate_content_tokens(result);
            if estimated_tokens > MAX_TOOL_RESULT_TOKENS {
                info!(
                    "Tool result from '{}' is {} tokens (budget {}), summarizing with disk reference",
                    tool_name, estimated_tokens, MAX_TOOL_RESULT_TOKENS
                );
                summarize_and_spill(tool_name, call_id, result, estimated_tokens).await
            } else {
                result.to_string()
            }
        };

        // Redact any secrets (API keys, tokens, credentials) the tool output may
        // contain BEFORE it enters the model's conversation context — otherwise a
        // command that echoes a secret (or a mis-run `cat .env`) would leak
        // credentials into context and every downstream log/exfil path.
        let result_to_store = crate::safety::redact::redact_secrets(&result_to_store).into_owned();

        // Trust gate: scan untrusted tool output for prompt-injection
        // patterns and neutralize high-severity findings in place (the loop
        // cannot refuse a tool result, so the offending lines are replaced
        // and flagged instead of the result being dropped).
        let gate = trust_gate_tool_result(
            tool_name,
            args_str,
            &result_to_store,
            self.config.safety.trust_gate_tool_results,
        );
        if gate.sanitized > 0 {
            self.trust_gate_findings += gate.sanitized;
            warn!(
                "trust gate sanitized {} finding(s) in '{}' tool output: {}",
                gate.sanitized,
                tool_name,
                gate.kinds.join(", ")
            );
        }
        let result_to_store = gate.content;

        if use_native_fc {
            let result_json = if success {
                result_to_store
            } else {
                serde_json::json!({"error": result_to_store}).to_string()
            };
            self.messages.push(Message::tool(result_json, call_id));
        } else {
            let formatted = if success {
                format!("<tool_result>{}</tool_result>", result_to_store)
            } else {
                format!(
                    "<tool_result><error>{}</error></tool_result>",
                    result_to_store
                )
            };
            self.messages.push(Message::user(formatted));
        }
    }

    pub(super) fn log_tool_call(
        &mut self,
        tool_name: &str,
        arguments: &str,
        result: &str,
        success: bool,
        start_time: std::time::Instant,
        truncate_result: bool,
    ) {
        let duration_ms = start_time.elapsed().as_millis() as u64;
        self.log_session_tool_call_event(
            tool_name,
            arguments,
            result,
            success,
            duration_ms,
            truncate_result,
        );

        if let Some(ref mut checkpoint) = self.current_checkpoint {
            let logged_result = if truncate_result {
                result.chars().take(1000).collect()
            } else {
                result.to_string()
            };
            checkpoint.log_tool_call(ToolCallLog {
                timestamp: chrono::Utc::now(),
                tool_name: tool_name.to_string(),
                arguments: arguments.to_string(),
                result: Some(logged_result),
                success,
                duration_ms: Some(duration_ms),
            });
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/agent/tool_dispatch/tool_dispatch_test.rs"]
mod tests;
