use chrono::Utc;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

use super::*;
use crate::checkpoint::VisualAssertion;
use crate::cognitive::CyclePhase;

/// Result of visual verification including whether it should hard-gate execution.
pub(super) struct VisualVerificationResult {
    /// Message to append to the tool result (always present on non-pass).
    pub message: String,
    /// True when the verification failed with high confidence and should block.
    pub hard_failure: bool,
    /// The assertion record to log to the checkpoint.
    pub assertion: Option<VisualAssertion>,
}

const EXPECTED_VISUAL_ARG: &str = "expected_visual";

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

pub(super) fn is_capability_disclaimer_response(content: &str) -> bool {
    let lower = super::recovery::strip_think_blocks(content).to_lowercase();
    let capability_markers = [
        "execute external tools",
        "execute tools",
        "execute system commands",
        "run external shell commands",
        "access local file system",
        "access local file systems",
        "access the file system",
        "access files on your local system",
        "run tools",
        "view images directly",
        "call tools",
        "interact with vision analysis tools",
        "analyze the image",
        "visual analysis of its specific contents",
        "only generate text responses",
        "only process and respond to the text",
        "information provided to me",
        "information provided directly",
    ];
    let refusal_markers = [
        "as an ai text model",
        "as a text model",
        "do not have the capability",
        "don't have the capability",
        "cannot fulfill this request",
        "cannot provide a visual analysis",
        "cannot provide visual analysis",
        "i cannot",
        "i can't",
        "unable to",
    ];
    let capability_hits = capability_markers
        .iter()
        .filter(|marker| lower.contains(**marker))
        .count();
    if capability_hits >= 2 {
        return true;
    }

    refusal_markers.iter().any(|marker| lower.contains(*marker)) && capability_hits >= 1
}

pub(super) fn exact_response_target(task: &str) -> Option<String> {
    let task = task.trim();
    let lower = task.to_lowercase();
    let prefixes = [
        "reply with exactly this text and nothing else:",
        "respond with exactly this text and nothing else:",
        "answer with exactly this text and nothing else:",
    ];

    for prefix in prefixes {
        if lower.starts_with(prefix) {
            let target = task[prefix.len()..].trim();
            if !target.is_empty() {
                return Some(target.to_string());
            }
        }
    }

    None
}

pub(super) fn matches_exact_response_target(content: &str, target: &str) -> bool {
    super::recovery::strip_think_blocks(content).trim() == target
}

/// Detect responses that describe future work instead of delivering a completed result.
/// This catches false completions like "I need to read the tests first" or pseudo-tool
/// plans embedded in plain text.
pub(super) fn is_incomplete_action_response(content: &str) -> bool {
    let lower = super::recovery::strip_think_blocks(content)
        .trim()
        .to_lowercase();
    if lower.is_empty() {
        return false;
    }

    let strong_prefixes = [
        "i need to ",
        "first i need to ",
        "first, i need to ",
        "let me ",
        "before i can ",
        "the next step is to ",
        "to continue, i need to ",
    ];
    if strong_prefixes
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        return true;
    }

    let strong_markers = [
        "i need to read",
        "i need to inspect",
        "i need to review",
        "i need to understand",
        "i need to look at",
        "before making changes",
        "before i can fix",
        "before i can implement",
        "file_read(",
        "file_read:",
        "file_edit(",
        "file_edit:",
        "file_write(",
        "file_write:",
        "shell_exec(",
        "shell_exec:",
    ];

    strong_markers.iter().any(|marker| lower.contains(marker))
}

fn truncate_visual_note(input: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut chars = input.chars();
    for _ in 0..max_chars {
        let Some(ch) = chars.next() else {
            return out;
        };
        out.push(ch);
    }
    if chars.next().is_some() {
        out.push_str("...");
    }
    out
}

fn visual_verification_expectation(tool_name: &str, args: &Value) -> Option<String> {
    if let Some(expected) = args.get(EXPECTED_VISUAL_ARG).and_then(|v| v.as_str()) {
        let trimmed = expected.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    if tool_name != "computer_window" {
        return None;
    }

    match args.get("action").and_then(|v| v.as_str()) {
        Some("launch") => args
            .get("app_name")
            .and_then(|v| v.as_str())
            .map(|app_name| {
                format!(
                    "A visible {} application window should now be open and usable on screen.",
                    app_name
                )
            }),
        Some("focus") => Some(
            "The requested application window should now be focused and clearly visible on screen."
                .to_string(),
        ),
        _ => None,
    }
}

fn configured_visual_verifier(
    config: &crate::config::Config,
) -> Option<crate::testing::visual_verification::VisualVerifier> {
    let profile = config
        .models
        .get("vision")
        .or_else(|| config.resolve_model(None))?;

    if !profile.supports_vision() {
        return None;
    }

    Some(crate::testing::visual_verification::VisualVerifier::from_model_profile(profile))
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

    fn diff_paths_for_completion_gate(&self) -> Option<Vec<String>> {
        let root = super::current_project_root();
        let output = std::process::Command::new("git")
            .args(["diff", "--name-only", "--"])
            .current_dir(root)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        Some(
            stdout
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
        )
    }

    fn gate_path_is_test(path: &str) -> bool {
        let lower = path.trim_matches('"').to_ascii_lowercase();
        let parts: Vec<&str> = lower.split('/').filter(|part| !part.is_empty()).collect();
        if parts
            .iter()
            .any(|part| matches!(*part, "test" | "tests" | "__tests__" | "spec" | "specs"))
        {
            return true;
        }

        let basename = parts.last().copied().unwrap_or(lower.as_str());
        let stem = basename
            .rsplit_once('.')
            .map(|(stem, _)| stem)
            .unwrap_or(basename);
        stem == "test"
            || stem == "spec"
            || stem.starts_with("test_")
            || stem.starts_with("test-")
            || stem.ends_with("_test")
            || stem.ends_with("-test")
            || stem.ends_with("_spec")
            || stem.ends_with("-spec")
            || basename.contains(".test.")
            || basename.contains(".spec.")
    }

    fn gate_path_is_source(path: &str) -> bool {
        let lower = path.trim_matches('"').to_ascii_lowercase();
        let Some(ext) = std::path::Path::new(&lower)
            .extension()
            .and_then(|e| e.to_str())
        else {
            return false;
        };
        matches!(
            ext,
            "py" | "js"
                | "jsx"
                | "ts"
                | "tsx"
                | "java"
                | "cs"
                | "c"
                | "cc"
                | "cpp"
                | "cxx"
                | "h"
                | "hh"
                | "hpp"
                | "sql"
                | "go"
                | "swift"
                | "rs"
        )
    }

    fn mutation_completion_gate(&self) -> Option<String> {
        if !super::tool_dispatch::task_requires_mutation(self.learning_context()) {
            return None;
        }

        if let Some(paths) = self.diff_paths_for_completion_gate() {
            if paths.is_empty() {
                return Some(
                    "EmptyDiff: this task requires a code change, but `git diff` is empty. \
                     Edit the relevant source file before completing."
                        .to_string(),
                );
            }

            let has_source_edit = paths
                .iter()
                .any(|path| Self::gate_path_is_source(path) && !Self::gate_path_is_test(path));
            let all_test_files = paths.iter().all(|path| Self::gate_path_is_test(path));

            if all_test_files {
                return Some(format!(
                    "TestOnlyPatch: the current diff only modifies test files ({:?}). \
                     SWE-style repair tasks require a source-code fix. Edit the implementation file before completing.",
                    paths
                ));
            }

            if !has_source_edit {
                return Some(format!(
                    "NoSourceEdit: the current diff does not include a supported source file ({:?}). \
                     Edit source code in Python, JavaScript, TypeScript, Java, C#, C/C++, SQL, Go, Swift, or Rust before completing.",
                    paths
                ));
            }
        } else if self.mutating_tool_call_count() == 0 {
            return Some(
                "EmptyDiff: this task requires a code change, but no mutating tool has succeeded. \
                 Edit a source file before completing."
                    .to_string(),
            );
        }

        if self.mutation_sequence > 0
            && self.last_successful_verification_mutation_sequence < self.mutation_sequence
        {
            if let Some(summary) = &self.last_failed_verification_summary {
                return Some(format!(
                    "FailingTestsAccepted: the latest verification after your edit failed: {summary}. \
                     Fix the issue and run verification again before completing."
                ));
            }
            return Some(
                "StaleVerification: verification has not passed after the most recent source edit. \
                 Run the project's relevant verification command after your last change before completing."
                    .to_string(),
            );
        }

        None
    }

    /// Check whether the agent has done enough work to accept completion.
    /// Returns `None` to accept, or `Some(message)` to reject with instructions.
    pub(super) fn check_completion_gate(&self) -> Option<String> {
        let context_target =
            (!self.current_task_context.is_empty()).then_some(self.current_task_context.as_str());
        let literal_target = self
            .current_checkpoint
            .as_ref()
            .map(|cp| cp.task_description.as_str())
            .or(context_target)
            .and_then(exact_response_target);

        let missing_required_tools = self.missing_required_task_tools();
        if !missing_required_tools.is_empty() {
            let required_tool_list = missing_required_tools
                .iter()
                .map(|tool| format!("`{}`", tool))
                .collect::<Vec<_>>()
                .join(", ");
            return Some(format!(
                "This task explicitly requires {} before you may answer. Call the required tool now and use its result. Do NOT answer from memory, filenames, or prior knowledge.",
                required_tool_list
            ));
        }

        let step_count = self.loop_control.current_step();
        let min_steps = self.config.agent.min_completion_steps;
        let skip_min_steps_for_read_only = !self.current_task_context.is_empty()
            && !super::tool_dispatch::task_requires_mutation(self.learning_context());

        if step_count < min_steps && !skip_min_steps_for_read_only {
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

        if is_incomplete_action_response(&self.last_assistant_response) {
            return Some(
                "Your response describes work you still need to do instead of a completed result. \
                 Do NOT stop to narrate your next step. Call the needed tool now and continue."
                    .to_string(),
            );
        }

        if let Some(target) = literal_target.as_deref() {
            if !matches_exact_response_target(&self.last_assistant_response, target) {
                return Some(format!(
                    "This task requires an exact literal response. Reply with exactly `{}` and nothing else.",
                    target
                ));
            }
        }

        if is_capability_disclaimer_response(&self.last_assistant_response) {
            return Some(
                "Your response incorrectly claims you cannot use tools, the filesystem, or image analysis. \
                 Use the tools that are available and answer directly from their results instead of giving a capability disclaimer."
                    .to_string(),
            );
        }

        // Reject completion if the last assistant response contains code that
        // should have been written to a file. This catches the common pattern
        // where models output code as text instead of using file_write/file_edit.
        if super::execution::contains_unwritten_code(&self.last_assistant_response) {
            return Some(
                "Your response contains code that was NOT written to any file. \
                 Use file_write to save it to a file, then verify with cargo check. \
                 Do NOT output code as text — use tools."
                    .to_string(),
            );
        }

        // Workflow validator: reject test-only edits when task requires source changes
        if let Some(msg) = self.validate_workflow_edits() {
            return Some(msg);
        }

        if let Some(msg) = self.mutation_completion_gate() {
            return Some(msg);
        }

        // Reject completion when the task requires code changes but no source files
        // were written at all. This catches the "context insufficient" early-quit
        // pattern where the model gives a text-only answer without doing any work.
        if self.config.agent.require_verification_before_completion {
            let has_any_file_write = self
                .messages
                .iter()
                .filter(|m| m.role == "assistant")
                .filter_map(|m| m.tool_calls.as_ref())
                .flatten()
                .any(|tc| matches!(tc.function.name.as_str(), "file_edit" | "file_write"));

            if !has_any_file_write {
                let task_desc = self
                    .current_checkpoint
                    .as_ref()
                    .map(|cp| cp.task_description.to_lowercase())
                    .unwrap_or_default();
                let task_requires_code = task_desc.contains("implement")
                    || task_desc.contains("create")
                    || task_desc.contains("build")
                    || task_desc.contains("write")
                    || task_desc.contains("fix")
                    || task_desc.contains("add")
                    || task_desc.contains("make");

                if task_requires_code {
                    return Some(
                        "You have not written or edited ANY files yet. The task requires you to \
                         write code. Use file_write or file_edit to create the implementation, \
                         then run cargo test to verify. Do NOT give up or say context is insufficient \
                         — read the files and start coding."
                            .to_string(),
                    );
                }
            }
        }

        if self.config.agent.require_verification_before_completion {
            // For non-mutation/non-Rust tasks, skip the legacy cargo-only
            // gate. Mutation-required tasks are handled by
            // mutation_completion_gate(), which is language-aware and requires
            // verification after the latest edit.
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

    /// Detect when the agent only edited test files without modifying source code.
    /// This catches a common failure pattern where models write tests instead of fixes.
    fn validate_workflow_edits(&self) -> Option<String> {
        // Scan message history for successful file_edit/file_write tool results
        // This is more reliable than checkpoints since messages are always up-to-date
        let edited_files: Vec<String> = self
            .messages
            .iter()
            .filter(|m| m.role == "assistant")
            .filter_map(|m| m.tool_calls.as_ref())
            .flatten()
            .filter(|tc| matches!(tc.function.name.as_str(), "file_edit" | "file_write"))
            .filter_map(|tc| {
                serde_json::from_str::<serde_json::Value>(&tc.function.arguments)
                    .ok()
                    .and_then(|v| {
                        v.get("path")
                            .and_then(|p| p.as_str().map(|s| s.to_string()))
                    })
            })
            .collect();

        debug!(
            "Workflow validator: found {} edited files from message history: {:?}",
            edited_files.len(),
            edited_files
        );

        // No file edits → no validation needed
        if edited_files.is_empty() {
            return None;
        }

        // Check if ALL edited files look like test files
        let test_patterns = [
            "test_", "tests/", "tests.", "_test.", "_test/", "spec/", "spec.", "_spec.",
        ];
        let all_test_files = edited_files.iter().all(|path| {
            let lower = path.to_lowercase();
            test_patterns.iter().any(|p| lower.contains(p))
        });

        // Check if the task description suggests source modification is needed
        let task_desc = self
            .current_checkpoint
            .as_ref()
            .map(|cp| cp.task_description.to_lowercase())
            .unwrap_or_default();
        let needs_source_change = task_desc.contains("fix")
            || task_desc.contains("bug")
            || task_desc.contains("implement")
            || task_desc.contains("modify")
            || task_desc.contains("change")
            || task_desc.contains("update")
            || task_desc.contains("patch")
            || task_desc.contains("source code");

        // Reject test-only edits when task requires source changes
        if all_test_files && needs_source_change {
            warn!(
                "Workflow validator: only test files edited ({:?}), task requires source changes",
                edited_files
            );
            let files_str = edited_files.join(", ");
            return Some(format!(
                "You only modified test files ({files_str}) but the task requires fixing SOURCE CODE. \
                 Do NOT only write tests. You MUST edit the actual source file(s) that contain the bug. \
                 Read the relevant source file, find the bug, and use file_edit to fix it."
            ));
        }

        // Also reject test-only edits if no source files were edited at all
        // (unless the task is explicitly about writing tests)
        if all_test_files && !needs_source_change {
            // Check if task is explicitly about writing tests
            let task_lower = task_desc.to_lowercase();
            let is_test_task = task_lower.contains("test")
                && (task_lower.contains("write")
                    || task_lower.contains("add")
                    || task_lower.contains("create")
                    || task_lower.contains("coverage")
                    || task_lower.contains("regression")
                    || task_lower.contains("reproducer")
                    || task_lower.contains("fix test")
                    || task_lower.contains("update test")
                    || task_lower.contains("improve"));

            if !is_test_task {
                warn!(
                    "Workflow validator: only test files edited ({:?}), no source files modified",
                    edited_files
                );
                let files_str = edited_files.join(", ");
                return Some(format!(
                    "You only modified test files ({files_str}) but did not edit any source files. \
                     If this task requires code changes, you MUST edit the actual source file(s). \
                     If this is a test-writing task, ensure you're also updating source code if needed."
                ));
            }
        }

        if !all_test_files {
            debug!("Workflow validator: source files edited, task OK");
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
                    self.last_successful_verification_mutation_sequence = self.mutation_sequence;
                    self.last_failed_verification_summary = None;
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
                    let summary = report
                        .checks
                        .iter()
                        .find(|check| !check.passed)
                        .map(|check| {
                            let output: String = check.output.chars().take(300).collect();
                            format!("{} failed: {}", check.check_type.as_str(), output)
                        })
                        .unwrap_or_else(|| "verification failed".to_string());
                    self.last_failed_verification_summary = Some(summary);
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
                self.last_failed_verification_summary =
                    Some(format!("verification could not run: {}", e));
                None
            }
        }
    }

    pub(super) async fn maybe_verify_visual_change(
        &mut self,
        tool_name: &str,
        args: &Value,
    ) -> Option<VisualVerificationResult> {
        if !matches!(
            tool_name,
            "computer_mouse" | "computer_keyboard" | "computer_window"
        ) {
            return None;
        }

        let expectation = visual_verification_expectation(tool_name, args)?;
        let verifier = configured_visual_verifier(&self.config)?;

        info!(
            "Running visual verification after {} with expectation: {}",
            tool_name, expectation
        );
        self.cognitive_state.set_phase(CyclePhase::Verify);
        let spinner = crate::ui::spinner::TerminalSpinner::start("Visual verifying...");

        let captured = match crate::computer::screen::ScreenCapture::capture_full().await {
            Ok(captured) => captured,
            Err(e) => {
                spinner.stop_error("Visual verification unavailable");
                let msg = format!(
                    "Visual verification could not capture the screen after `{}`: {}",
                    tool_name,
                    truncate_visual_note(&e.to_string(), 160)
                );
                warn!("{}", msg);
                self.push_task_state_note(msg.clone());
                self.pending_failure_hint = Some(format!(
                    "Visual verification could not capture the screen after `{}`. Re-check the UI manually or retry with `computer_screen` before continuing.",
                    tool_name
                ));
                return Some(VisualVerificationResult {
                    message: format!(
                        "\n\n<visual_verification_unavailable>\n{}\n</visual_verification_unavailable>",
                        msg
                    ),
                    hard_failure: false,
                    assertion: None,
                });
            }
        };

        let current_step = self.loop_control.current_step();

        // Save screenshot to durable storage and compute SHA-256 hash for forensics
        let screenshot_result: Option<(std::path::PathBuf, String)> = {
            use base64::Engine as _;
            match base64::engine::general_purpose::STANDARD.decode(&captured.base64_png) {
                Ok(png_bytes) => {
                    // Compute SHA-256 hash of raw screenshot bytes for stable hashing
                    let sha_hash = {
                        let mut hasher = Sha256::new();
                        hasher.update(&png_bytes);
                        format!("{:x}", hasher.finalize())
                    };

                    // Also track with simple hash for basic stuck-loop detection
                    let simple_hash = super::recovery::hash_text_signature(&sha_hash);
                    let _ = self.detect_visual_stuck_loop(simple_hash);

                    // Build durable evidence directory: ~/.selfware/visual_evidence/{task_id}/
                    let task_id = self
                        .current_checkpoint
                        .as_ref()
                        .map(|cp| cp.task_id.clone())
                        .unwrap_or_else(|| "unknown".to_string());
                    let evidence_dir = dirs::home_dir()
                        .unwrap_or_else(|| std::path::PathBuf::from("."))
                        .join(".selfware")
                        .join("visual_evidence")
                        .join(&task_id);

                    match std::fs::create_dir_all(&evidence_dir) {
                        Ok(()) => {
                            let timestamp = Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
                            let filename = format!("step_{}_{}.png", current_step, timestamp);
                            let filepath = evidence_dir.join(&filename);
                            match std::fs::write(&filepath, &png_bytes) {
                                Ok(()) => Some((filepath, sha_hash)),
                                Err(e) => {
                                    warn!(
                                        "Failed to write screenshot to {}: {}",
                                        filepath.display(),
                                        e
                                    );
                                    None
                                }
                            }
                        }
                        Err(e) => {
                            warn!(
                                "Failed to create evidence dir {}: {}",
                                evidence_dir.display(),
                                e
                            );
                            None
                        }
                    }
                }
                Err(_) => None,
            }
        };

        let require_hard_gate = self.config.agent.require_visual_verification;

        match verifier
            .verify_screenshot(&captured.base64_png, &expectation)
            .await
        {
            Ok(report) if report.passed => {
                spinner.stop_success("Visual verification passed");
                self.push_task_state_note(format!(
                    "Visual verification passed after `{}` ({:.0}% confidence)",
                    tool_name,
                    report.confidence * 100.0
                ));
                let (screenshot_path, screenshot_hash) = screenshot_result
                    .as_ref()
                    .map(|(p, h)| (Some(p.clone()), h.clone()))
                    .unwrap_or((None, String::new()));
                let assertion = VisualAssertion {
                    id: format!(
                        "va-{}-{}",
                        current_step,
                        uuid::Uuid::new_v4()
                            .to_string()
                            .split('-')
                            .next()
                            .unwrap_or("")
                    ),
                    description: expectation.clone(),
                    screenshot_path,
                    verified: false,
                    verification_result: Some(crate::session::checkpoint::VerificationResult {
                        passed: true,
                        confidence: report.confidence as f32,
                        explanation: report.description.clone(),
                        screenshot_hash,
                    }),
                    created_at: Utc::now(),
                    verified_at: None,
                    step: Some(current_step),
                    tool_name: Some(tool_name.to_string()),
                    expected: Some(expectation.clone()),
                    observed: Some(report.description.clone()),
                    passed: Some(true),
                    confidence: Some(report.confidence),
                    screenshot_hash_legacy: None,
                    timestamp: Some(Utc::now()),
                };
                Some(VisualVerificationResult {
                    message: String::new(),
                    hard_failure: false,
                    assertion: Some(assertion),
                })
            }
            Ok(report) => {
                spinner.stop_error("Visual verification failed");
                let issues = if report.issues.is_empty() {
                    "No specific mismatches listed".to_string()
                } else {
                    report.issues.join("; ")
                };
                let note = format!(
                    "Visual verification failed after `{}`: expected `{}`, observed `{}`",
                    tool_name,
                    truncate_visual_note(&expectation, 120),
                    truncate_visual_note(&report.description, 120)
                );
                self.push_task_state_note(note);
                self.pending_failure_hint = Some(format!(
                    "Visual verification after `{}` did not match the expected UI state. Expected: {}. Observed: {}. Issues: {}. Re-check the screen before continuing.",
                    tool_name,
                    truncate_visual_note(&expectation, 200),
                    truncate_visual_note(&report.description, 200),
                    truncate_visual_note(&issues, 200)
                ));
                let hard_failure = require_hard_gate && report.confidence > 0.6;
                let message = if hard_failure {
                    format!(
                        "\n\n<visual_verification_failed hard_gate=\"true\">\nVISUAL VERIFICATION HARD FAILURE — this action did NOT produce the expected result.\nexpected: {}\nobserved: {}\nconfidence: {:.2}\nissues: {}\nYou MUST retry this action or take a different approach before continuing.\n</visual_verification_failed>",
                        expectation,
                        report.description,
                        report.confidence,
                        issues
                    )
                } else {
                    format!(
                        "\n\n<visual_verification_failed>\nexpected: {}\nobserved: {}\nconfidence: {:.2}\nissues: {}\n</visual_verification_failed>",
                        expectation,
                        report.description,
                        report.confidence,
                        issues
                    )
                };
                let (screenshot_path, screenshot_hash) = screenshot_result
                    .as_ref()
                    .map(|(p, h)| (Some(p.clone()), h.clone()))
                    .unwrap_or((None, String::new()));
                let assertion = VisualAssertion {
                    id: format!(
                        "va-{}-{}",
                        current_step,
                        uuid::Uuid::new_v4()
                            .to_string()
                            .split('-')
                            .next()
                            .unwrap_or("")
                    ),
                    description: expectation.clone(),
                    screenshot_path,
                    verified: true,
                    verification_result: Some(crate::session::checkpoint::VerificationResult {
                        passed: false,
                        confidence: report.confidence as f32,
                        explanation: report.description.clone(),
                        screenshot_hash,
                    }),
                    created_at: Utc::now(),
                    verified_at: Some(Utc::now()),
                    step: Some(current_step),
                    tool_name: Some(tool_name.to_string()),
                    expected: Some(expectation.clone()),
                    observed: Some(report.description.clone()),
                    passed: Some(false),
                    confidence: Some(report.confidence),
                    screenshot_hash_legacy: None,
                    timestamp: Some(Utc::now()),
                };
                Some(VisualVerificationResult {
                    message,
                    hard_failure,
                    assertion: Some(assertion),
                })
            }
            Err(e) => {
                spinner.stop_error("Visual verification unavailable");
                let msg = format!(
                    "Visual verification request failed after `{}`: {}",
                    tool_name,
                    truncate_visual_note(&e.to_string(), 160)
                );
                warn!("{}", msg);
                self.push_task_state_note(msg.clone());
                self.pending_failure_hint = Some(format!(
                    "Visual verification could not complete after `{}`. Verify the screen with `computer_screen` or troubleshoot the vision endpoint before continuing.",
                    tool_name
                ));
                Some(VisualVerificationResult {
                    message: format!(
                        "\n\n<visual_verification_unavailable>\n{}\n</visual_verification_unavailable>",
                        msg
                    ),
                    hard_failure: false,
                    assertion: None,
                })
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn explicit_visual_expectation_takes_priority() {
        let args = json!({
            "action": "click",
            "expected_visual": "A confirmation dialog should be visible."
        });
        assert_eq!(
            visual_verification_expectation("computer_mouse", &args).as_deref(),
            Some("A confirmation dialog should be visible.")
        );
    }

    #[test]
    fn computer_window_launch_has_default_expectation() {
        let args = json!({
            "action": "launch",
            "app_name": "Firefox"
        });
        let expectation = visual_verification_expectation("computer_window", &args).unwrap();
        assert!(expectation.contains("Firefox"));
        assert!(expectation.contains("visible"));
    }

    #[test]
    fn non_window_actions_without_expectation_skip_visual_gate() {
        let args = json!({
            "action": "type",
            "text": "hello"
        });
        assert!(visual_verification_expectation("computer_keyboard", &args).is_none());
    }
}
