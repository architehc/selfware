use serde_json::Value;
use tracing::{debug, info, warn};

use super::*;
use crate::cognitive::CyclePhase;

/// Detect responses that contain framework self-reference instead of task output.
/// Returns true if the content references multiple internal implementation details,
/// indicating the model is confused and reasoning about the framework itself.
pub(super) fn is_confused_response(content: &str) -> bool {
    let markers = [
        "</think>",
        "selfware_system_directive",
        "build_no_action_prompt_message",
        "should_prompt_for_action",
        "maybe_prompt_for_action",
        "ActionPrompt::",
    ];
    let lower = content.to_lowercase();
    markers
        .iter()
        .filter(|m| lower.contains(&m.to_lowercase()))
        .count()
        >= 2
}

impl Agent {
    /// Tool categories that inherently bypass the Rust/cargo verification gate.
    /// These tools indicate non-Rust tasks (browser automation, vision analysis,
    /// desktop control, web fetching, etc.) where `cargo check` is meaningless.
    pub(crate) const NON_RUST_TOOL_PREFIXES: &'static [&'static str] = &[
        "browser_",  // browser_fetch, browser_screenshot, browser_pdf, browser_eval, browser_links
        "vision_",   // vision_analyze, vision_compare
        "computer_", // computer_mouse, computer_keyboard, computer_screen, computer_window
        "screen_capture", // screen_capture
        "page_control", // page_control (screenshot, click, type, scroll, etc.)
        "http_request", // http_request
    ];

    /// Tools that are read-only / informational and never modify code.
    /// Tasks that only use these tools should not require cargo verification.
    const READ_ONLY_TOOLS: &'static [&'static str] = &[
        "file_read",
        "directory_tree",
        "glob_find",
        "grep_search",
        "symbol_search",
        "git_status",
        "git_diff",
        "git_log",
        "lsp_goto_definition",
        "lsp_find_references",
        "lsp_document_symbols",
        "lsp_hover",
        "context_status",
        "context_focus",
        "context_recommend",
        "context_bulk_read",
        "context_summary",
        "context_load_skeleton",
        "knowledge_query",
        "knowledge_stats",
        "knowledge_export",
        "process_list",
        "process_logs",
        "port_check",
    ];

    /// Returns true if the current task appears to be a non-Rust task that should
    /// bypass cargo-based verification.  Three conditions trigger the bypass:
    ///
    /// 1. **No Cargo.toml** in the working directory — there is no Rust project to verify.
    /// 2. **Only non-Rust tools used** — the task exclusively used browser, vision,
    ///    computer-control, or web tools with no file-write or cargo activity.
    /// 3. **Only read-only tools used** — the task only read files, searched, or
    ///    queried information without making any changes. No code was modified,
    ///    so there is nothing to verify.
    pub(super) fn should_skip_cargo_verification(&self) -> bool {
        // Condition 1: No Cargo.toml in the project root or its ancestors → not a Rust project
        if !super::current_project_root().join("Cargo.toml").exists() {
            debug!(
                "Completion gate: no Cargo.toml found in project ancestors, skipping cargo verification"
            );
            return true;
        }

        let Some(cp) = self.current_checkpoint.as_ref() else {
            return false;
        };

        // If there are no tool calls at all, this is a text-only response — skip cargo
        if cp.tool_calls.is_empty() {
            debug!("Completion gate: no tool calls in checkpoint, skipping cargo verification");
            return true;
        }

        // Condition 2: Every tool call is a non-Rust tool
        let all_non_rust = cp.tool_calls.iter().all(|tc| {
            Self::NON_RUST_TOOL_PREFIXES
                .iter()
                .any(|prefix| tc.tool_name.starts_with(prefix))
        });

        if all_non_rust {
            debug!(
                "Completion gate: all tool calls are non-Rust tools, skipping cargo verification"
            );
            return true;
        }

        // Condition 3: Every tool call is read-only (no code was modified)
        let all_read_only = cp.tool_calls.iter().all(|tc| {
            Self::READ_ONLY_TOOLS.contains(&tc.tool_name.as_str())
                || Self::NON_RUST_TOOL_PREFIXES
                    .iter()
                    .any(|prefix| tc.tool_name.starts_with(prefix))
        });

        if all_read_only {
            debug!(
                "Completion gate: all {} tool calls are read-only, skipping cargo verification",
                cp.tool_calls.len()
            );
            return true;
        }

        false
    }

    /// Check whether the agent has done enough work to accept completion.
    /// Returns `None` to accept, or `Some(message)` to reject with instructions.
    pub(super) fn check_completion_gate(&self) -> Option<String> {
        let step_count = self.loop_control.current_step();
        let min_steps = self.config.agent.min_completion_steps;

        if step_count < min_steps {
            // Tailor the message: don't mention cargo for non-Rust tasks
            let verification_hint = if self.should_skip_cargo_verification() {
                "Continue working: review your results and ensure the task is fully complete."
            } else {
                "Continue working: verify your changes compile with cargo_check and pass tests with cargo_test."
            };
            return Some(format!(
                "You are trying to complete the task after only {} step(s), but at least {} are required. \
                 You have a large budget — do not rush. {}",
                step_count, min_steps, verification_hint
            ));
        }

        if self.config.agent.require_verification_before_completion {
            // Skip cargo verification entirely for non-Rust tasks
            if self.should_skip_cargo_verification() {
                debug!("Completion gate: bypassing cargo verification for non-Rust task");
                return None;
            }

            let has_verification = self
                .current_checkpoint
                .as_ref()
                .map(|cp| {
                    cp.tool_calls.iter().any(|tc| {
                        tc.success
                            && matches!(
                                tc.tool_name.as_str(),
                                "cargo_check" | "cargo_test" | "cargo_clippy"
                            )
                    })
                })
                .unwrap_or(false);

            if !has_verification {
                return Some(
                    "You must run at least one verification tool (cargo_check, cargo_test, or cargo_clippy) \
                     successfully before completing the task. Please verify your work now."
                        .to_string(),
                );
            }
        }

        None
    }

    pub(super) async fn maybe_verify_file_change(
        &mut self,
        tool_name: &str,
        args: &Value,
    ) -> Option<String> {
        if !matches!(tool_name, "file_edit" | "file_write") {
            return None;
        }

        let path = args.get("path").and_then(|v| v.as_str())?;
        info!("Running verification after {} on {}", tool_name, path);
        self.cognitive_state.set_phase(CyclePhase::Verify);
        let spinner = crate::ui::spinner::TerminalSpinner::start("Verifying...");

        match self
            .verification_gate
            .verify_change(&[path.to_string()], &format!("{}:{}", tool_name, path))
            .await
        {
            Ok(report) => {
                if report.overall_passed {
                    spinner.stop_success("Verification passed");
                    self.cognitive_state.episodic_memory.what_worked(
                        tool_name,
                        &format!("{} on {} passed verification", tool_name, path),
                    );
                    if crate::output::is_verbose() {
                        crate::output::verification_report(&format!("{}", report), true);
                    }
                    None
                } else {
                    spinner.stop_error("Verification failed");
                    self.cognitive_state.episodic_memory.what_failed(
                        tool_name,
                        &format!("{} on {} failed verification", tool_name, path),
                    );
                    crate::output::verification_report(&format!("{}", report), false);
                    Some(format!(
                        "\n\n<verification_failed>\n{}\n</verification_failed>",
                        report
                    ))
                }
            }
            Err(e) => {
                spinner.stop_error("Verification failed to run");
                warn!("Verification failed to run: {}", e);
                None
            }
        }
    }

    pub(super) fn maybe_enhance_tool_result(&self, name: &str, result_str: &str) -> String {
        if name == "cargo_check" && result_str.contains("\"success\":false") {
            self.enhance_cargo_errors(result_str)
        } else {
            result_str.to_string()
        }
    }
}
