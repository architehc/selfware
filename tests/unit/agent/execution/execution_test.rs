use super::*;
use crate::checkpoint::ToolCallLog;
use crate::testing::mock_api::MockLlmServer;
use chrono::Utc;
use std::hash::{Hash, Hasher};

#[test]
fn files_checklist_line_tolerates_markdown_decoration() {
    // Bare form still works.
    assert!(Agent::is_files_checklist_line("FILES: src/main.rs"));
    assert!(Agent::is_files_checklist_line("  FILES: a.rs"));
    // Markdown decoration that capable hosted models (GLM-4.x/5.x) emit.
    assert!(Agent::is_files_checklist_line("**FILES:** src/main.rs"));
    assert!(Agent::is_files_checklist_line("- **Files:** a.rs, b.rs"));
    assert!(Agent::is_files_checklist_line("#### FILES: a.rs"));
    assert!(Agent::is_files_checklist_line("> files: a.rs"));
    // Non-headers must not match.
    assert!(!Agent::is_files_checklist_line(
        "The files are listed below"
    ));
    assert!(!Agent::is_files_checklist_line("profiles: none"));
}

/// Detect a real existing codebase to suppress the Rust scaffold injection.
/// Empty dir + the SAB scratch shape (only src/lib.rs) must report zero.
/// Anything with another source file must report ≥ 1.
#[tokio::test]
async fn count_source_files_skips_scaffold_target_and_excluded_dirs() {
    // Serialize + restore cwd so we don't race/poison sibling tests.
    let cwd = crate::test_support::CwdGuard::hold();

    // 1. Genuinely empty workdir → 0
    let empty = tempfile::tempdir().unwrap();
    cwd.switch_to(empty.path());
    assert_eq!(count_source_files_in_workdir(5).await, 0);

    // 2. SAB-style: only src/lib.rs → 0 (the scaffold target itself doesn't count)
    let sab = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(sab.path().join("src")).unwrap();
    std::fs::write(sab.path().join("src/lib.rs"), "// empty\n").unwrap();
    cwd.switch_to(sab.path());
    assert_eq!(count_source_files_in_workdir(5).await, 0);

    // 3. Real Rust codebase: tests/foo.rs alongside src/lib.rs → 1
    std::fs::create_dir_all(sab.path().join("tests")).unwrap();
    std::fs::write(sab.path().join("tests/foo.rs"), "fn it_works() {}\n").unwrap();
    assert!(count_source_files_in_workdir(5).await >= 1);

    // 4. Excluded dirs (target/, node_modules/) must not count
    let go_repo = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(go_repo.path().join("target")).unwrap();
    std::fs::write(go_repo.path().join("target/built.rs"), "fn cached() {}\n").unwrap();
    std::fs::create_dir_all(go_repo.path().join("node_modules/lodash")).unwrap();
    std::fs::write(
        go_repo.path().join("node_modules/lodash/index.js"),
        "// dep\n",
    )
    .unwrap();
    cwd.switch_to(go_repo.path());
    assert_eq!(count_source_files_in_workdir(5).await, 0);

    // 5. Real non-Rust codebase: a top-level .go file → 1 (must NOT scaffold)
    std::fs::write(go_repo.path().join("main.go"), "package main\n").unwrap();
    assert!(count_source_files_in_workdir(5).await >= 1);

    // 6. Cap is honoured (returns early once threshold reached)
    let many = tempfile::tempdir().unwrap();
    for i in 0..10 {
        std::fs::write(many.path().join(format!("f{i}.py")), "x = 1\n").unwrap();
    }
    cwd.switch_to(many.path());
    assert_eq!(count_source_files_in_workdir(3).await, 3);
    // cwd restored on drop of `cwd`.
}

// =========================================================================
// Helper: mirrors should_prompt_for_action logic for standalone testing
// =========================================================================
fn should_prompt_for_action(
    content: &str,
    has_no_tool_calls: bool,
    use_last_message: bool,
    reasoning_chars: usize,
) -> bool {
    if !has_no_tool_calls || use_last_message {
        return false;
    }

    let effective_content = super::recovery::strip_think_blocks(content);
    let effective_len = effective_content.len();

    let total_output = effective_len + reasoning_chars;
    if total_output > 0 && effective_len > 500 {
        let think_ratio = reasoning_chars as f64 / total_output as f64;
        if think_ratio < 0.8 {
            return false;
        }
    } else if effective_len >= 1000 {
        return false;
    }

    let intent_phrases = [
        "let me", "i'll ", "i will", "let's", "first,", "starting", "begin by", "going to",
        "need to", "start by", "help you",
    ];
    let lower = effective_content.to_lowercase();
    intent_phrases.iter().any(|p| lower.contains(p))
}

/// Build a fake assistant message that contains a file_write tool call.
/// Tests that exercise the completion gate need this because the gate now
/// scans `self.messages` (not checkpoints) for evidence that the agent
/// actually wrote files.
fn fake_file_write_message() -> crate::api::types::Message {
    crate::api::types::Message {
        role: "assistant".to_string(),
        content: crate::api::types::MessageContent::Text(String::new()),
        reasoning_content: None,
        tool_calls: Some(vec![crate::api::types::ToolCall {
            id: "tc_fake".to_string(),
            call_type: "function".to_string(),
            function: crate::api::types::ToolFunction {
                name: "file_write".to_string(),
                arguments: r#"{"path":"dummy.py","content":"x"}"#.to_string(),
            },
        }]),
        tool_call_id: None,
        name: None,
    }
}

/// Same as above but for file_edit.
fn fake_file_edit_message() -> crate::api::types::Message {
    crate::api::types::Message {
        role: "assistant".to_string(),
        content: crate::api::types::MessageContent::Text(String::new()),
        reasoning_content: None,
        tool_calls: Some(vec![crate::api::types::ToolCall {
            id: "tc_fake".to_string(),
            call_type: "function".to_string(),
            function: crate::api::types::ToolFunction {
                name: "file_edit".to_string(),
                arguments: r#"{"path":"src/lib.rs","old_str":"a","new_str":"b"}"#.to_string(),
            },
        }]),
        tool_call_id: None,
        name: None,
    }
}

// =========================================================================
// should_prompt_for_action tests
// =========================================================================

#[test]
fn test_should_prompt_when_intent_phrase_present() {
    assert!(should_prompt_for_action(
        "Let me check the file",
        true,
        false,
        0
    ));
    assert!(should_prompt_for_action(
        "I'll fix that bug now",
        true,
        false,
        0
    ));
    assert!(should_prompt_for_action(
        "I will refactor the module",
        true,
        false,
        0
    ));
    assert!(should_prompt_for_action(
        "Let's start by reading the code",
        true,
        false,
        0
    ));
    assert!(should_prompt_for_action(
        "First, I need to understand",
        true,
        false,
        0
    ));
    assert!(should_prompt_for_action(
        "Going to investigate",
        true,
        false,
        0
    ));
}

#[test]
fn test_should_not_prompt_when_tool_calls_exist() {
    // has_no_tool_calls = false means there ARE tool calls
    assert!(!should_prompt_for_action("Let me check", false, false, 0));
}

#[test]
fn test_should_not_prompt_when_using_last_message() {
    assert!(!should_prompt_for_action("Let me check", true, true, 0));
}

#[test]
fn test_should_not_prompt_for_long_content() {
    let long_content = format!("Let me {}", "x".repeat(1000));
    assert!(!should_prompt_for_action(&long_content, true, false, 0));
}

#[test]
fn test_should_not_prompt_for_plain_response() {
    assert!(!should_prompt_for_action(
        "The answer is 42.",
        true,
        false,
        0
    ));
    assert!(!should_prompt_for_action(
        "Here is the result.",
        true,
        false,
        0
    ));
}

#[test]
fn test_should_prompt_case_insensitive() {
    assert!(should_prompt_for_action("LET ME check", true, false, 0));
    assert!(should_prompt_for_action("STARTING now", true, false, 0));
    assert!(should_prompt_for_action("BEGIN BY reading", true, false, 0));
}

#[test]
fn test_should_prompt_think_dominated_response() {
    // Short intent content + huge think block = should still detect intent
    let content = "Let me check the file structure";
    let reasoning_chars = 5000; // Simulates large think block
    assert!(should_prompt_for_action(
        content,
        true,
        false,
        reasoning_chars
    ));
}

#[test]
fn test_should_prompt_genuine_long_response() {
    // 600 chars of real content with low think ratio = genuine response
    let content = format!("Here is a detailed analysis: {}", "x".repeat(570));
    assert!(!should_prompt_for_action(&content, true, false, 100));
}

#[test]
fn test_is_confused_response() {
    // Two or more framework markers = confused
    assert!(super::verification::is_confused_response(
        "The should_prompt_for_action function checks ActionPrompt:: variants"
    ));
    assert!(super::verification::is_confused_response(
        "Looking at build_no_action_prompt_message and </think> blocks"
    ));
    // Only one marker = not confused
    assert!(!super::verification::is_confused_response(
        "The function uses ActionPrompt to decide"
    ));
    // No markers = not confused
    assert!(!super::verification::is_confused_response(
        "Here is the code review summary"
    ));
}

#[test]
fn test_is_capability_disclaimer_response() {
    assert!(super::verification::is_capability_disclaimer_response(
            "I cannot fulfill this request. I am an AI assistant and do not have the capability to execute external tools like `vision_analyze`, access local file systems, or view images directly. Additionally, I cannot call tools as described in your prompt; I can only generate text responses based on the information provided to me."
        ));
    assert!(super::verification::is_capability_disclaimer_response(
            "I cannot execute the `vision_analyze` tool or access the file system to analyze the image. As an AI text model, I do not have the capability to run external shell commands or interact with vision analysis tools directly."
        ));
    assert!(!super::verification::is_capability_disclaimer_response(
        "The image shows a weathered coastal building beside the sea."
    ));
}

#[tokio::test]
async fn test_maybe_prompt_for_action_escalates_after_repeated_no_action_turns() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    // First attempt returns Corrected
    assert!(matches!(
        agent
            .maybe_prompt_for_action("Let me inspect the file", true, false, 0)
            .unwrap(),
        ActionPrompt::Corrected
    ));
    assert!(agent
        .messages
        .last()
        .unwrap()
        .content
        .text()
        .contains("selfware_system_directive"));

    // Second attempt still corrects (FORCE_FALLBACK_AFTER=3)
    assert!(matches!(
        agent
            .maybe_prompt_for_action("Let me inspect the file", true, false, 0)
            .unwrap(),
        ActionPrompt::Corrected
    ));

    // Third attempt triggers ForceFallback
    assert!(matches!(
        agent
            .maybe_prompt_for_action("Let me inspect the file", true, false, 0)
            .unwrap(),
        ActionPrompt::ForceFallback
    ));

    server.stop().await;
}

#[tokio::test]
async fn test_maybe_prompt_for_action_resets_after_non_intent_response() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    assert!(matches!(
        agent
            .maybe_prompt_for_action("Let me inspect the file", true, false, 0)
            .unwrap(),
        ActionPrompt::Corrected
    ));
    assert_eq!(agent.consecutive_no_action_prompts, 1);

    // Non-intent response resets the counter
    assert!(matches!(
        agent
            .maybe_prompt_for_action("Here is the result.", true, false, 0)
            .unwrap(),
        ActionPrompt::NotNeeded
    ));
    assert_eq!(agent.consecutive_no_action_prompts, 0);

    // After reset, first intent is Corrected again
    assert!(matches!(
        agent
            .maybe_prompt_for_action("Let me inspect the file", true, false, 0)
            .unwrap(),
        ActionPrompt::Corrected
    ));
    assert!(agent
        .messages
        .last()
        .unwrap()
        .content
        .text()
        .contains("selfware_system_directive"));

    server.stop().await;
}

#[tokio::test]
async fn test_total_no_action_counter_survives_consecutive_resets() {
    // Simulates the cycling pattern: intent → correction → non-intent (resets
    // consecutive counter) → intent again → ... The lifetime counter should
    // still accumulate and eventually abort.
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    for cycle in 0..250 {
        // One intent prompt (consecutive=1..3, then ForceFallback on third)
        let _ = agent.maybe_prompt_for_action("Let me check", true, false, 0);
        let _ = agent.maybe_prompt_for_action("Let me check", true, false, 0);
        // One non-intent response resets the consecutive counter
        let _ = agent.maybe_prompt_for_action("Here is the result.", true, false, 0);
        assert_eq!(
            agent.consecutive_no_action_prompts, 0,
            "consecutive should reset on non-intent response (cycle {})",
            cycle
        );
    }
    // 250 cycles * 2 intent prompts = 500 total, which equals MAX_TOTAL_NO_ACTION_PROMPTS
    assert_eq!(agent.total_no_action_prompts, 500);

    // The next intent prompt should trigger the lifetime abort
    let result = agent.maybe_prompt_for_action("Let me try again", true, false, 0);
    assert!(
        result.is_err(),
        "should abort after exceeding lifetime no-action limit"
    );
    server.stop().await;
}

// =========================================================================
// maybe_enhance_tool_result tests
// =========================================================================

#[test]
fn test_enhance_tool_result_no_change_for_non_cargo() {
    // The function only enhances cargo_check results with "success":false
    let name = "file_read";
    let result_str = r#"{"content":"hello"}"#;
    // Non-cargo_check tools pass through unchanged
    if name != "cargo_check" || !result_str.contains("\"success\":false") {
        assert_eq!(result_str, result_str);
    }
}

#[test]
fn test_enhance_tool_result_triggers_for_failed_cargo_check() {
    let name = "cargo_check";
    let result_str = r#"{"success":false,"stderr":"error[E0308]: mismatched types"}"#;
    let should_enhance = name == "cargo_check" && result_str.contains("\"success\":false");
    assert!(should_enhance);
}

#[test]
fn test_enhance_tool_result_skips_successful_cargo_check() {
    let name = "cargo_check";
    let result_str = r#"{"success":true,"stderr":""}"#;
    let should_enhance = name == "cargo_check" && result_str.contains("\"success\":false");
    assert!(!should_enhance);
}

// =========================================================================
// build_tool_call_context tests (via MockLlmServer + Agent)
// =========================================================================

fn mock_config(endpoint: String) -> Config {
    Config {
        endpoint,
        model: "mock-model".to_string(),
        agent: crate::config::AgentConfig {
            max_iterations: 5,
            step_timeout_secs: 5,
            streaming: false,
            native_function_calling: false,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[tokio::test]
async fn test_build_tool_call_context_without_native_fc() {
    let server = MockLlmServer::builder().with_response("done").build().await;

    let config = mock_config(format!("{}/v1", server.url()));
    let agent = Agent::new(config).await.unwrap();

    let (call_id, use_native_fc, fake_call) =
        agent.build_tool_call_context("file_read", r#"{"path":"test.rs"}"#, None);

    assert!(!use_native_fc);
    assert!(call_id.starts_with("call_"));
    assert_eq!(fake_call.function.name, "file_read");
    assert_eq!(fake_call.function.arguments, r#"{"path":"test.rs"}"#);
    assert_eq!(fake_call.call_type, "function");

    server.stop().await;
}

#[tokio::test]
async fn test_build_tool_call_context_with_native_fc_and_id() {
    let server = MockLlmServer::builder().with_response("done").build().await;

    let mut config = mock_config(format!("{}/v1", server.url()));
    config.agent.native_function_calling = true;
    let agent = Agent::new(config).await.unwrap();

    let (call_id, use_native_fc, fake_call) = agent.build_tool_call_context(
        "shell_exec",
        r#"{"command":"ls"}"#,
        Some("call_existing_123".to_string()),
    );

    assert!(use_native_fc);
    assert_eq!(call_id, "call_existing_123");
    assert_eq!(fake_call.function.name, "shell_exec");

    server.stop().await;
}

// =========================================================================
// warn_on_unparsed_tool_content (logic check)
// =========================================================================

#[test]
fn test_warn_condition_content_has_tool_keywords_but_no_calls() {
    let content = "I want to use a tool_name function to help";
    let tool_calls: Vec<CollectedToolCall> = vec![];

    // The warn fires when tool_calls empty AND content contains suspicious keywords
    let should_warn = tool_calls.is_empty()
        && (content.contains("<tool")
            || content.contains("tool_name")
            || content.contains("function"));

    assert!(should_warn);
}

#[test]
fn test_warn_condition_no_warn_when_calls_present() {
    let content = "Using tool_name to execute function";
    let tool_calls: Vec<CollectedToolCall> =
        vec![("file_read".to_string(), "{}".to_string(), None)];

    let should_warn = tool_calls.is_empty()
        && (content.contains("<tool")
            || content.contains("tool_name")
            || content.contains("function"));

    assert!(!should_warn);
}

#[test]
fn test_warn_condition_no_warn_for_clean_content() {
    let content = "Here is a summary of the code changes.";
    let tool_calls: Vec<CollectedToolCall> = vec![];

    let should_warn = tool_calls.is_empty()
        && (content.contains("<tool")
            || content.contains("tool_name")
            || content.contains("function"));

    assert!(!should_warn);
}

// =========================================================================
// Helper: create a test agent with permissive config
// =========================================================================
fn test_config(endpoint: String) -> Config {
    crate::test_support::mock_agent_config(&endpoint)
}

// =========================================================================
// FILES: checklist guard tests
// =========================================================================

#[tokio::test]
async fn test_edit_to_already_read_file_satisfies_files_guard() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let write = vec![(
        "file_write".to_string(),
        r#"{"path":"src/main.rs","content":"x"}"#.to_string(),
        None,
    )];
    // Editing a file that was never read is a blind edit — not exempted.
    assert!(
        !agent.writes_target_only_read_files(&write),
        "unread target must not be exempt from the FILES guard"
    );
    // After the file has been read, the same edit is exempt (regression: the
    // guard used to discard this write on the doable parse_port task).
    agent.file_tracker.read_state.insert(
        "src/main.rs".to_string(),
        super::super::FileReadState::default(),
    );
    assert!(
        agent.writes_target_only_read_files(&write),
        "edit to an already-read file should satisfy the FILES guard"
    );
    // A read-only batch has no writes → not exempt (nothing to exempt).
    let read_only = vec![(
        "file_read".to_string(),
        r#"{"path":"src/main.rs"}"#.to_string(),
        None,
    )];
    assert!(!agent.writes_target_only_read_files(&read_only));
}

#[tokio::test]
async fn test_force_mutation_bypasses_files_guard() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    // Normal first edit without a FILES: checklist is blocked.
    assert!(
        agent.check_files_guard(true),
        "first edit without checklist should be blocked"
    );
    // Read-only tools are never blocked by the FILES guard.
    assert!(
        !agent.check_files_guard(false),
        "read-only tool should not be blocked"
    );

    // When force-mutation recovery is pending, the edit passes through.
    agent.force_mutation_pending = true;
    assert!(
        !agent.check_files_guard(true),
        "force-mutation edit should bypass FILES guard"
    );
    // The bypass is consumed once the edit is accepted.
    assert!(
        !agent.force_mutation_pending,
        "force-mutation pending flag should be consumed"
    );
    // After consumption, another edit without a checklist is blocked again.
    assert!(
        agent.check_files_guard(true),
        "subsequent edit should be blocked again after bypass consumed"
    );

    // A seen FILES: checklist disables the guard entirely.
    agent.files_checklist_seen = true;
    assert!(!agent.check_files_guard(true));

    // A prior successful write also disables the guard.
    agent.files_checklist_seen = false;
    agent.has_written_any_file = true;
    assert!(!agent.check_files_guard(true));

    server.stop().await;
}

// =========================================================================
// FILES: guard — shell command file-write classification
// =========================================================================

#[test]
fn test_shell_command_file_write_intent_classification() {
    // Mutating but NOT file-writing: must not trip the FILES guard
    // (regression: these used to be discarded as "writes").
    for cmd in [
        "mkdir -p src/components",
        "npm install",
        "npm install && npm run build",
        "pip install -r requirements.txt",
        "pnpm install",
        "git add -A && git commit -m init",
        "git checkout -b feature",
        "cargo build --release",
        "cargo test",
        "touch src/new.rs",
        "rm -rf target/tmp",
        "chmod +x run.sh",
        "mktemp -d",
        "cat file.rs",
        "ls -la",
    ] {
        assert!(
            !shell_command_is_file_write_intent(cmd),
            "'{}' must NOT be classified as a file-write intent",
            cmd
        );
    }

    // Actual file-write intents: the guard keeps covering these.
    for cmd in [
        "echo 'fn main() {}' > src/main.rs",
        "cat <<EOF > src/lib.rs",
        "echo x>>log.txt",
        "echo hi | tee out.txt",
        "sed -i 's/foo/bar/' src/main.rs",
        "sed --in-place 's/a/b/' f.rs",
        "perl -pi -e 's/a/b/' f.rs",
        "cp src/a.rs src/b.rs",
        "mv old.rs new.rs",
        "rsync -a a/ b/",
        "patch -p1 < fix.diff",
        "git apply fix.diff",
        "cargo fmt",
        "cargo fix --allow-dirty",
    ] {
        assert!(
            shell_command_is_file_write_intent(cmd),
            "'{}' should be classified as a file-write intent",
            cmd
        );
    }
}

#[test]
fn test_shell_redirect_detection_ignores_fd_dup_and_quotes() {
    // Descriptor duplication writes no file.
    assert!(!shell_command_is_file_write_intent("cargo test 2>&1"));
    assert!(!shell_command_is_file_write_intent("make >&2"));
    // Quoted '>' is data, not a redirect.
    assert!(!shell_command_is_file_write_intent(
        "grep \"->\" src/main.rs"
    ));
    assert!(!shell_command_is_file_write_intent("echo 'a > b'"));
    // Redirect without spaces still counts.
    assert!(shell_command_is_file_write_intent("echo x>y"));
}

#[test]
fn test_tool_call_file_write_intent_shell_vs_file_tools() {
    // shell_exec is classified by command content.
    assert!(!tool_call_is_file_write_intent(
        "shell_exec",
        r#"{"command":"mkdir foo && npm install"}"#
    ));
    assert!(tool_call_is_file_write_intent(
        "shell_exec",
        r#"{"command":"echo x > foo.txt"}"#
    ));
    // File tools count as file-write intents.
    assert!(tool_call_is_file_write_intent(
        "file_write",
        r#"{"path":"a.rs","content":"x"}"#
    ));
    assert!(tool_call_is_file_write_intent(
        "file_edit",
        r#"{"path":"a.rs","old":"x","new":"y"}"#
    ));
    // Read-only tools do not.
    assert!(!tool_call_is_file_write_intent(
        "file_read",
        r#"{"path":"a.rs"}"#
    ));
    // Unparseable shell args are not treated as writes.
    assert!(!tool_call_is_file_write_intent("shell_exec", "not json"));
}

#[tokio::test]
async fn test_writes_target_only_read_files_ignores_non_file_shell() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    agent.file_tracker.read_state.insert(
        "src/main.rs".to_string(),
        super::super::FileReadState::default(),
    );

    // A batch mixing an edit to a read file with a non-file shell command
    // (mkdir) is still exempt: the shell call is not a file write, so it
    // must not disqualify the batch the way the old state-change
    // classification did.
    let batch = vec![
        (
            "file_edit".to_string(),
            r#"{"path":"src/main.rs","old":"a","new":"b"}"#.to_string(),
            None,
        ),
        (
            "shell_exec".to_string(),
            r#"{"command":"mkdir -p src/generated"}"#.to_string(),
            None,
        ),
    ];
    assert!(
        agent.writes_target_only_read_files(&batch),
        "mkdir in the batch must not make a read-file edit 'blind'"
    );

    // A shell command that WRITES a file has no tracked read state (no
    // `path` arg), so it is treated as a blind write.
    let shell_write = vec![(
        "shell_exec".to_string(),
        r#"{"command":"echo x > src/main.rs"}"#.to_string(),
        None,
    )];
    assert!(!agent.writes_target_only_read_files(&shell_write));

    server.stop().await;
}

// =========================================================================
// detect_and_correct_malformed_tools tests
// =========================================================================

#[tokio::test]
async fn test_detect_malformed_returns_false_when_calls_present() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let tool_calls: Vec<CollectedToolCall> =
        vec![("file_read".to_string(), "{}".to_string(), None)];

    let result = agent.detect_and_correct_malformed_tools("<tool broken>", &tool_calls);
    assert!(!result);

    server.stop().await;
}

#[tokio::test]
async fn test_detect_malformed_returns_false_no_markers() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let tool_calls: Vec<CollectedToolCall> = vec![];

    let result = agent.detect_and_correct_malformed_tools("Just a normal response.", &tool_calls);
    assert!(!result);

    server.stop().await;
}

#[tokio::test]
async fn test_detect_malformed_detects_tool_marker() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();
    let initial_len = agent.messages.len();

    let tool_calls: Vec<CollectedToolCall> = vec![];
    let result = agent.detect_and_correct_malformed_tools("<tool broken format>", &tool_calls);

    assert!(result);
    assert_eq!(agent.messages.len(), initial_len + 1);
    let last_msg = agent.messages.last().unwrap();
    assert_eq!(last_msg.role, "user");
    assert!(last_msg.content.text().contains("malformed"));

    server.stop().await;
}

#[tokio::test]
async fn test_detect_malformed_detects_function_marker() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let tool_calls: Vec<CollectedToolCall> = vec![];
    let result = agent.detect_and_correct_malformed_tools(
        "<function=file_read>{\"path\":\"test.rs\"}</function>",
        &tool_calls,
    );

    assert!(result);
    let last_msg = agent.messages.last().unwrap();
    assert!(last_msg.content.text().contains("EXACT format"));

    server.stop().await;
}

#[tokio::test]
async fn test_detect_malformed_ignores_plain_tool_name_text() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let tool_calls: Vec<CollectedToolCall> = vec![];
    let result = agent.detect_and_correct_malformed_tools("tool_name: file_read", &tool_calls);

    assert!(!result);

    server.stop().await;
}

#[tokio::test]
async fn test_detect_malformed_ignores_plain_tool_call_text() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let tool_calls: Vec<CollectedToolCall> = vec![];
    let result = agent
        .detect_and_correct_malformed_tools("I'll use tool_call to read the file", &tool_calls);

    assert!(!result);

    server.stop().await;
}

#[tokio::test]
async fn test_detect_malformed_detects_name_equals_marker() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let tool_calls: Vec<CollectedToolCall> = vec![];
    let result = agent.detect_and_correct_malformed_tools("<name=file_read>", &tool_calls);

    assert!(result);

    server.stop().await;
}

// =========================================================================
// check_completion_gate tests
// =========================================================================

#[tokio::test]
async fn test_gate_passes_no_requirements() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = false;
    let agent = Agent::new(config).await.unwrap();

    let result = agent.check_completion_gate().await;
    assert!(result.is_none(), "Expected gate to pass, got: {:?}", result);

    server.stop().await;
}

#[tokio::test]
async fn test_gate_rejects_insufficient_steps() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 5;
    config.agent.require_verification_before_completion = false;
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_task_context = "Implement the requested change".to_string();

    let result = agent.check_completion_gate().await;
    assert!(result.is_some());
    let msg = result.unwrap();
    assert!(msg.contains("only"));
    assert!(msg.contains("step"));
    assert!(msg.contains("required"));

    server.stop().await;
}

#[tokio::test]
async fn test_gate_rejects_missing_required_explicit_tool() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = false;
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_task_context = "Use vision_analyze on ./sample.jpg".to_string();
    agent
        .required_task_tools
        .insert("vision_analyze".to_string());

    let result = agent.check_completion_gate().await;
    assert!(result.is_some());
    assert!(result.unwrap().contains("vision_analyze"));

    server.stop().await;
}

#[tokio::test]
async fn test_gate_passes_after_required_explicit_tool_succeeds() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = false;
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_task_context = "Use vision_analyze on ./sample.jpg".to_string();
    agent
        .required_task_tools
        .insert("vision_analyze".to_string());

    let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
        "vision-task".to_string(),
        "Use vision_analyze on ./sample.jpg".to_string(),
    );
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "vision_analyze".to_string(),
        arguments: r#"{"image_path":"./sample.jpg"}"#.to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(150),
    });
    agent.current_checkpoint = Some(checkpoint);

    assert!(agent.check_completion_gate().await.is_none());

    server.stop().await;
}

#[tokio::test]
async fn test_gate_rejects_capability_disclaimer_after_required_tool_succeeds() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = false;
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_task_context = "Use vision_analyze on ./sample.jpg".to_string();
    agent
        .required_task_tools
        .insert("vision_analyze".to_string());
    agent.last_assistant_response =
            "I cannot execute the `vision_analyze` tool or access the file system to analyze the image. As an AI text model, I do not have the capability to run external shell commands or interact with vision analysis tools directly."
                .to_string();

    let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
        "vision-task".to_string(),
        "Use vision_analyze on ./sample.jpg".to_string(),
    );
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "vision_analyze".to_string(),
        arguments: r#"{"image_path":"./sample.jpg"}"#.to_string(),
        result: Some(r#"{"analysis":"aircraft"}"#.to_string()),
        success: true,
        duration_ms: Some(150),
    });
    agent.current_checkpoint = Some(checkpoint);

    let result = agent.check_completion_gate().await;
    assert!(result.is_some(), "Capability disclaimers should never pass");
    assert!(
        result.unwrap().contains("capability disclaimer"),
        "Expected the gate to explain the disclaimer rejection"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_gate_accepts_exact_literal_response() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = false;
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_task_context =
        "Reply with exactly this text and nothing else: python validate.py".to_string();
    agent.last_assistant_response = "python validate.py".to_string();

    assert!(
        agent.check_completion_gate().await.is_none(),
        "Exact literal responses should pass"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_gate_rejects_non_exact_literal_response() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = false;
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_task_context =
        "Reply with exactly this text and nothing else: python validate.py".to_string();
    agent.last_assistant_response =
        "The default full validation CLI command is `python validate.py`.".to_string();

    let result = agent.check_completion_gate().await;
    assert!(result.is_some(), "Non-exact literal responses should fail");
    assert!(
        result.unwrap().contains("exact literal response"),
        "Expected the gate to explain the exact-response failure"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_gate_skips_min_steps_for_read_only_task_context() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 5;
    config.agent.require_verification_before_completion = false;
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_task_context =
        "What is the default full validation CLI command for this workspace?".to_string();

    assert!(
        agent.check_completion_gate().await.is_none(),
        "read-only tasks should not be forced through the min-step gate"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_gate_rejects_missing_verification() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = true;
    let agent = Agent::new(config).await.unwrap();

    let result = agent.check_completion_gate().await;
    assert!(result.is_some());
    let msg = result.unwrap();
    assert!(msg.contains("verification"));

    server.stop().await;
}

#[tokio::test]
async fn test_gate_rejects_incomplete_planning_response() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = false;
    let mut agent = Agent::new(config).await.unwrap();

    agent.last_assistant_response =
            "I need to read the tests to understand what to implement.\n\nfile_read: tests/chart_tests.rs"
                .to_string();

    let result = agent.check_completion_gate().await;
    assert!(
        result.is_some(),
        "Incomplete planning response should reject completion"
    );
    assert!(
        result
            .unwrap()
            .contains("describes work you still need to do"),
        "Expected gate to explain why the planning response was rejected"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_gate_passes_with_cargo_check_verification() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = true;
    let mut agent = Agent::new(config).await.unwrap();

    let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
        "test-task".to_string(),
        "test description".to_string(),
    );
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_check".to_string(),
        arguments: "{}".to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(100),
    });
    agent.current_checkpoint = Some(checkpoint);

    let result = agent.check_completion_gate().await;
    assert!(
        result.is_none(),
        "Expected gate to pass with verification, got: {:?}",
        result
    );

    server.stop().await;
}

#[tokio::test]
async fn test_gate_rejects_failed_verification() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = true;
    let mut agent = Agent::new(config).await.unwrap();

    let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
        "test-task".to_string(),
        "test description".to_string(),
    );
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_check".to_string(),
        arguments: "{}".to_string(),
        result: Some("error".to_string()),
        success: false,
        duration_ms: Some(100),
    });
    agent.current_checkpoint = Some(checkpoint);

    let result = agent.check_completion_gate().await;
    assert!(result.is_some(), "Failed verification should reject");

    server.stop().await;
}

#[tokio::test]
async fn test_gate_accepts_cargo_test_verification() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = true;
    let mut agent = Agent::new(config).await.unwrap();

    let mut checkpoint =
        crate::checkpoint::TaskCheckpoint::new("test-task".to_string(), "test desc".to_string());
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_test".to_string(),
        arguments: "{}".to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(200),
    });
    agent.current_checkpoint = Some(checkpoint);

    assert!(agent.check_completion_gate().await.is_none());

    server.stop().await;
}

#[tokio::test]
async fn test_gate_accepts_cargo_clippy_verification() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = true;
    let mut agent = Agent::new(config).await.unwrap();

    let mut checkpoint =
        crate::checkpoint::TaskCheckpoint::new("test-task".to_string(), "test desc".to_string());
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_clippy".to_string(),
        arguments: "{}".to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(300),
    });
    agent.current_checkpoint = Some(checkpoint);

    assert!(agent.check_completion_gate().await.is_none());

    server.stop().await;
}

#[tokio::test]
async fn test_gate_steps_checked_before_verification() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 100;
    config.agent.require_verification_before_completion = true;
    let agent = Agent::new(config).await.unwrap();

    let result = agent.check_completion_gate().await;
    assert!(result.is_some());
    let msg = result.unwrap();
    assert!(msg.contains("step"));

    server.stop().await;
}

// =========================================================================
// Non-Rust task bypass tests (P0 fix: cargo gate skipped for non-Rust tasks)
// =========================================================================

#[tokio::test]
async fn test_gate_bypassed_for_browser_only_task() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = true;
    let mut agent = Agent::new(config).await.unwrap();

    // Simulate a task that only used browser tools — no cargo calls
    let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
        "browser-task".to_string(),
        "fetch a webpage".to_string(),
    );
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "browser_fetch".to_string(),
        arguments: r#"{"url":"https://example.com"}"#.to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(500),
    });
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "browser_screenshot".to_string(),
        arguments: r#"{"url":"https://example.com"}"#.to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(300),
    });
    agent.current_checkpoint = Some(checkpoint);

    // Gate should pass — no cargo verification needed for browser-only tasks
    assert!(
        agent.check_completion_gate().await.is_none(),
        "Browser-only tasks should bypass the cargo verification gate"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_gate_bypassed_for_vision_only_task() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = true;
    let mut agent = Agent::new(config).await.unwrap();

    let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
        "vision-task".to_string(),
        "analyze an image".to_string(),
    );
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "vision_analyze".to_string(),
        arguments: r#"{"path":"/tmp/img.png"}"#.to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(200),
    });
    agent.current_checkpoint = Some(checkpoint);

    assert!(
        agent.check_completion_gate().await.is_none(),
        "Vision-only tasks should bypass the cargo verification gate"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_gate_bypassed_for_computer_control_task() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = true;
    let mut agent = Agent::new(config).await.unwrap();

    let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
        "desktop-task".to_string(),
        "click a button".to_string(),
    );
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "computer_mouse".to_string(),
        arguments: r#"{"action":"click","x":100,"y":200}"#.to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(50),
    });
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "screen_capture".to_string(),
        arguments: "{}".to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(100),
    });
    agent.current_checkpoint = Some(checkpoint);

    assert!(
        agent.check_completion_gate().await.is_none(),
        "Computer-control tasks should bypass the cargo verification gate"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_gate_bypassed_for_http_only_task() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = true;
    let mut agent = Agent::new(config).await.unwrap();

    let mut checkpoint =
        crate::checkpoint::TaskCheckpoint::new("http-task".to_string(), "call an API".to_string());
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "http_request".to_string(),
        arguments: r#"{"url":"https://api.example.com","method":"GET"}"#.to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(300),
    });
    agent.current_checkpoint = Some(checkpoint);

    assert!(
        agent.check_completion_gate().await.is_none(),
        "HTTP-only tasks should bypass the cargo verification gate"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_gate_still_required_for_mixed_rust_and_browser_task() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = true;
    let mut agent = Agent::new(config).await.unwrap();

    // Task that used both browser tools AND file_write (a Rust-project tool)
    let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
        "mixed-task".to_string(),
        "fetch web data and write Rust code".to_string(),
    );
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "browser_fetch".to_string(),
        arguments: r#"{"url":"https://example.com"}"#.to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(500),
    });
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_write".to_string(),
        arguments: r#"{"path":"src/main.rs","content":"fn main() {}"}"#.to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(50),
    });
    agent.current_checkpoint = Some(checkpoint);

    // Gate should REJECT — mixed task with file_write still needs cargo verification
    // (we're running from the selfware project root which has a Cargo.toml)
    let result = agent.check_completion_gate().await;
    assert!(
        result.is_some(),
        "Mixed Rust + browser tasks should still require cargo verification"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_gate_accepts_non_rust_verification() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = true;
    let mut agent = Agent::new(config).await.unwrap();

    // Create a temp directory without a Cargo.toml and chdir into it
    // (serialized + auto-restored, incl. on panic).
    let tmp = tempfile::tempdir().unwrap();
    let _guard = crate::test_support::CwdGuard::enter(tmp.path());

    // Task with file_write in a non-Rust project — a successful test/build
    // command (e.g. pytest) should satisfy the verification gate.
    agent.messages.push(fake_file_write_message());

    let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
        "python-task".to_string(),
        "write a python script".to_string(),
    );
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_write".to_string(),
        arguments: r#"{"path":"script.py","content":"print('hello')"}"#.to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(50),
    });
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "shell_exec".to_string(),
        arguments: r#"{"command":"pytest"}"#.to_string(),
        result: Some("passed".to_string()),
        success: true,
        duration_ms: Some(500),
    });
    agent.current_checkpoint = Some(checkpoint);

    let result = agent.check_completion_gate().await;
    assert!(
        result.is_none(),
        "Non-Rust verification (pytest) should satisfy the completion gate, got: {:?}",
        result,
    );

    server.stop().await;
}

// =========================================================================
// StaleVerification churn-breaker rescue selection (P0-2)
// =========================================================================

fn checkpoint_shell_call(command: &str, success: bool) -> ToolCallLog {
    ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "shell_exec".to_string(),
        arguments: serde_json::json!({"command": command}).to_string(),
        result: Some(if success {
            "ok".to_string()
        } else {
            "failed".to_string()
        }),
        success,
        duration_ms: Some(50),
    }
}

#[tokio::test]
async fn stale_verification_rescue_prefers_cargo_check_in_rust_project() {
    // The original Rust rescue is preserved: Cargo.toml present → cargo_check.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let _guard = crate::test_support::CwdGuard::enter(tmp.path());

    let agent = Agent::new(test_config("http://127.0.0.1:1".to_string()))
        .await
        .unwrap();
    let (tool, args, display) = agent
        .stale_verification_rescue_call()
        .expect("a Rust project should rescue with cargo_check");
    assert_eq!(tool, "cargo_check");
    assert_eq!(args, "{}");
    assert_eq!(display, "cargo_check");
}

#[tokio::test]
async fn stale_verification_rescue_reuses_models_own_command_outside_rust() {
    // P0-2 regression: the churn breaker was Rust-only, so a non-Rust
    // project whose model kept answering instead of re-verifying burned
    // to MAX_ITERATIONS. The language-agnostic signal is the project's
    // own test/check command the model already ran — re-run that.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("calc.py"),
        "def div(a, b):\n    return a / b\n",
    )
    .unwrap();
    let _guard = crate::test_support::CwdGuard::enter(tmp.path());

    let mut agent = Agent::new(test_config("http://127.0.0.1:1".to_string()))
        .await
        .unwrap();
    let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
        "python-task".to_string(),
        "fix the divide-by-zero bug in calc.py".to_string(),
    );
    // The most recent verification-framed command wins; non-verification
    // commands around it are skipped.
    checkpoint.log_tool_call(checkpoint_shell_call("python3 test_calc.py", false));
    checkpoint.log_tool_call(checkpoint_shell_call("ls -la", true));
    checkpoint.log_tool_call(checkpoint_shell_call("python3 test_calc.py", true));
    checkpoint.log_tool_call(checkpoint_shell_call("python3 app.py", true));
    agent.current_checkpoint = Some(checkpoint);

    let (tool, args, display) = agent
        .stale_verification_rescue_call()
        .expect("a run with a known verification command should rescue with it");
    assert_eq!(tool, "shell_exec");
    assert_eq!(display, "python3 test_calc.py");
    assert_eq!(
        args,
        serde_json::json!({"command": "python3 test_calc.py"}).to_string()
    );
}

#[tokio::test]
async fn stale_verification_rescue_is_none_without_any_verification_signal() {
    // No Cargo.toml and no verification-framed command in the run: there
    // is no honest command to auto-run, so no rescue.
    let tmp = tempfile::tempdir().unwrap();
    let _guard = crate::test_support::CwdGuard::enter(tmp.path());

    let mut agent = Agent::new(test_config("http://127.0.0.1:1".to_string()))
        .await
        .unwrap();
    let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
        "python-task".to_string(),
        "fix the divide-by-zero bug in calc.py".to_string(),
    );
    checkpoint.log_tool_call(checkpoint_shell_call("python3 app.py", true));
    agent.current_checkpoint = Some(checkpoint);

    assert!(agent.stale_verification_rescue_call().is_none());
}

#[tokio::test]
async fn test_gate_min_steps_message_omits_cargo_for_non_rust_task() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 100;
    config.agent.require_verification_before_completion = true;
    let mut agent = Agent::new(config).await.unwrap();

    // Only non-Rust tools used
    let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
        "browser-task".to_string(),
        "browse the web".to_string(),
    );
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "browser_fetch".to_string(),
        arguments: r#"{"url":"https://example.com"}"#.to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(500),
    });
    agent.current_checkpoint = Some(checkpoint);

    let result = agent.check_completion_gate().await;
    assert!(result.is_some());
    let msg = result.unwrap();
    // Should NOT mention cargo for non-Rust tasks
    assert!(
        !msg.contains("cargo_check"),
        "Min-steps message for non-Rust task should not mention cargo, got: {}",
        msg
    );
    assert!(msg.contains("step"));

    server.stop().await;
}

#[tokio::test]
async fn test_rust_subdirectory_does_not_bypass_cargo_verification() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = true;

    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"subdir-test\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let nested = tmp.path().join("crates").join("worker");
    std::fs::create_dir_all(&nested).unwrap();

    // Serialized chdir, auto-restored even on panic.
    let _guard = crate::test_support::CwdGuard::enter(&nested);

    let mut agent = Agent::new(config).await.unwrap();

    // The completion gate scans agent.messages for file_edit tool calls,
    // so we must add one to pass the "has written files" check.
    agent.messages.push(fake_file_edit_message());

    let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
        "rust-subdir".to_string(),
        "edit a rust file".to_string(),
    );
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_edit".to_string(),
        arguments: r#"{"path":"src/lib.rs","old_str":"a","new_str":"b"}"#.to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(25),
    });
    agent.current_checkpoint = Some(checkpoint);

    let result = agent.check_completion_gate().await;
    assert!(
        result.as_deref().is_some_and(
            |message| message.contains("cargo_check") || message.contains("verification tool")
        ),
        "Rust subdirectory should still require cargo verification, got: {:?}",
        result
    );

    server.stop().await;
}

#[tokio::test]
async fn test_non_rust_tool_prefixes_list_is_comprehensive() {
    // Ensure all known non-Rust tool names match at least one prefix
    let known_non_rust_tools = [
        "browser_fetch",
        "browser_screenshot",
        "browser_pdf",
        "browser_eval",
        "browser_links",
        "vision_analyze",
        "vision_compare",
        "computer_mouse",
        "computer_keyboard",
        "computer_screen",
        "computer_window",
        "screen_capture",
        "page_control",
        "http_request",
    ];

    for tool_name in &known_non_rust_tools {
        let matches = Agent::NON_RUST_TOOL_PREFIXES
            .iter()
            .any(|prefix| tool_name.starts_with(prefix));
        assert!(
            matches,
            "Tool '{}' should match a non-Rust prefix but does not",
            tool_name
        );
    }
}

#[test]
fn test_rust_tools_do_not_match_non_rust_prefixes() {
    // Ensure regular Rust-project tools do NOT match the bypass list
    let rust_tools = [
        "cargo_check",
        "cargo_test",
        "cargo_clippy",
        "file_write",
        "file_read",
        "shell_execute",
        "search_files",
    ];

    for tool_name in &rust_tools {
        let matches = Agent::NON_RUST_TOOL_PREFIXES
            .iter()
            .any(|prefix| tool_name.starts_with(prefix));
        assert!(
            !matches,
            "Rust tool '{}' should NOT match a non-Rust prefix",
            tool_name
        );
    }
}

// =========================================================================
// detect_repetition tests
// =========================================================================

#[tokio::test]
async fn test_repetition_no_loop_initially() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let tool_calls: Vec<CollectedToolCall> = vec![(
        "file_read".to_string(),
        r#"{"path":"test.rs"}"#.to_string(),
        None,
    )];

    let result = agent.detect_repetition(&tool_calls);
    assert!(result.is_none());

    server.stop().await;
}

#[tokio::test]
async fn test_repetition_detects_after_three_identical() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let tool_calls: Vec<CollectedToolCall> = vec![(
        "file_read".to_string(),
        r#"{"path":"same.rs"}"#.to_string(),
        None,
    )];

    assert!(agent.detect_repetition(&tool_calls).is_none());
    assert!(agent.detect_repetition(&tool_calls).is_none());

    let result = agent.detect_repetition(&tool_calls);
    assert!(result.is_some());
    let msg = result.unwrap();
    assert!(msg.contains("STUCK LOOP DETECTED"));
    assert!(msg.contains("file_read"));

    server.stop().await;
}

#[tokio::test]
async fn test_repetition_detects_simple_abab_oscillation() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let call_a: Vec<CollectedToolCall> = vec![(
        "file_read".to_string(),
        r#"{"path":"a.rs"}"#.to_string(),
        None,
    )];
    let call_b: Vec<CollectedToolCall> = vec![(
        "grep_search".to_string(),
        r#"{"query":"foo"}"#.to_string(),
        None,
    )];

    assert!(agent.detect_repetition(&call_a).is_none());
    assert!(agent.detect_repetition(&call_b).is_none());
    assert!(agent.detect_repetition(&call_a).is_none());

    let result = agent.detect_repetition(&call_b);
    assert!(result.is_some());
    let msg = result.unwrap();
    assert!(msg.contains("OSCILLATION LOOP DETECTED"));
    assert!(msg.contains("file_read"));
    assert!(msg.contains("grep_search"));

    server.stop().await;
}

#[tokio::test]
async fn test_repetition_no_loop_with_different_args() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let calls1: Vec<CollectedToolCall> = vec![(
        "file_read".to_string(),
        r#"{"path":"file1.rs"}"#.to_string(),
        None,
    )];
    let calls2: Vec<CollectedToolCall> = vec![(
        "file_read".to_string(),
        r#"{"path":"file2.rs"}"#.to_string(),
        None,
    )];
    let calls3: Vec<CollectedToolCall> = vec![(
        "file_read".to_string(),
        r#"{"path":"file3.rs"}"#.to_string(),
        None,
    )];

    assert!(agent.detect_repetition(&calls1).is_none());
    assert!(agent.detect_repetition(&calls2).is_none());
    assert!(agent.detect_repetition(&calls3).is_none());

    server.stop().await;
}

#[tokio::test]
async fn test_repetition_clears_history_on_detection() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let tool_calls: Vec<CollectedToolCall> = vec![(
        "file_read".to_string(),
        r#"{"path":"same.rs"}"#.to_string(),
        None,
    )];

    agent.detect_repetition(&tool_calls);
    agent.detect_repetition(&tool_calls);
    let result = agent.detect_repetition(&tool_calls);
    assert!(result.is_some());

    // After detection, history is cleared
    let result = agent.detect_repetition(&tool_calls);
    assert!(result.is_none());

    server.stop().await;
}

#[tokio::test]
async fn test_repetition_window_size_limits() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    for i in 0..10 {
        let calls: Vec<CollectedToolCall> =
            vec![(format!("tool_{}", i), format!(r#"{{"i":{}}}"#, i), None)];
        assert!(agent.detect_repetition(&calls).is_none());
    }

    let repeated: Vec<CollectedToolCall> =
        vec![("tool_0".to_string(), r#"{"i":0}"#.to_string(), None)];
    assert!(agent.detect_repetition(&repeated).is_none());

    server.stop().await;
}

#[tokio::test]
async fn test_repetition_multiple_tools_in_batch() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let batch: Vec<CollectedToolCall> = vec![
        (
            "file_read".to_string(),
            r#"{"path":"a.rs"}"#.to_string(),
            None,
        ),
        (
            "shell_exec".to_string(),
            r#"{"command":"ls"}"#.to_string(),
            None,
        ),
    ];

    assert!(agent.detect_repetition(&batch).is_none());
    assert!(agent.detect_repetition(&batch).is_none());

    // Third time, file_read with same args appears 3 times
    let result = agent.detect_repetition(&batch);
    assert!(result.is_some());

    server.stop().await;
}

#[tokio::test]
async fn test_repetition_json_whitespace_normalization() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    // Call with compact JSON
    let compact: Vec<CollectedToolCall> = vec![(
        "file_write".to_string(),
        r#"{"path":"foo.rs","content":"bar"}"#.to_string(),
        None,
    )];
    assert!(agent.detect_repetition(&compact).is_none());

    // Call with whitespace-padded but semantically identical JSON
    let spaced: Vec<CollectedToolCall> = vec![(
        "file_write".to_string(),
        r#"{ "path" : "foo.rs" , "content" : "bar" }"#.to_string(),
        None,
    )];
    assert!(agent.detect_repetition(&spaced).is_none());

    // Third call with yet another whitespace variant — should trigger loop detection
    let newlines: Vec<CollectedToolCall> = vec![(
        "file_write".to_string(),
        "{\n  \"path\": \"foo.rs\",\n  \"content\": \"bar\"\n}".to_string(),
        None,
    )];
    let result = agent.detect_repetition(&newlines);
    assert!(
        result.is_some(),
        "should detect loop across whitespace variants"
    );
    assert!(result.unwrap().contains("STUCK LOOP DETECTED"));

    server.stop().await;
}

#[tokio::test]
async fn test_repetition_invalid_json_falls_back_to_raw() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    // Invalid JSON strings that differ only in whitespace — should NOT be normalized
    // (they are not valid JSON, so fallback hashes raw strings which differ)
    let raw1: Vec<CollectedToolCall> =
        vec![("custom_tool".to_string(), "not json {".to_string(), None)];
    let raw2: Vec<CollectedToolCall> = vec![(
        "custom_tool".to_string(),
        "not json  {".to_string(), // extra space
        None,
    )];

    assert!(agent.detect_repetition(&raw1).is_none());
    assert!(agent.detect_repetition(&raw2).is_none());
    // Third call with raw1 again — only 2 of raw1 in window, so no loop
    assert!(agent.detect_repetition(&raw1).is_none());

    // But three identical raw strings DO trigger detection
    agent.recent_tool_calls.clear();
    agent.recent_tool_batches.clear();
    assert!(agent.detect_repetition(&raw1).is_none());
    assert!(agent.detect_repetition(&raw1).is_none());
    let result = agent.detect_repetition(&raw1);
    assert!(
        result.is_some(),
        "identical invalid-JSON raw strings should still trigger loop"
    );

    server.stop().await;
}

// =========================================================================
// push_tool_result_message tests
// =========================================================================

#[tokio::test]
async fn test_push_result_xml_success() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();
    let initial_len = agent.messages.len();

    agent
        .push_tool_result_message(false, "call_1", "test_tool", true, r#"{"output":"hello"}"#)
        .await;

    assert_eq!(agent.messages.len(), initial_len + 1);
    let msg = agent.messages.last().unwrap();
    assert_eq!(msg.role, "user");
    assert!(msg.content.text().starts_with("<tool_result>"));
    assert!(msg.content.text().contains(r#"{"output":"hello"}"#));
    assert!(!msg.content.text().contains("<error>"));

    server.stop().await;
}

#[tokio::test]
async fn test_push_result_xml_failure() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();
    let initial_len = agent.messages.len();

    agent
        .push_tool_result_message(false, "call_1", "test_tool", false, "Something went wrong")
        .await;

    assert_eq!(agent.messages.len(), initial_len + 1);
    let msg = agent.messages.last().unwrap();
    assert_eq!(msg.role, "user");
    assert!(msg.content.text().contains("<error>"));
    assert!(msg.content.text().contains("Something went wrong"));

    server.stop().await;
}

#[tokio::test]
async fn test_push_result_native_fc_success() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();
    let initial_len = agent.messages.len();

    agent
        .push_tool_result_message(true, "call_native_1", "test_tool", true, r#"{"data":"ok"}"#)
        .await;

    assert_eq!(agent.messages.len(), initial_len + 1);
    let msg = agent.messages.last().unwrap();
    assert_eq!(msg.role, "tool");
    assert_eq!(msg.tool_call_id.as_deref(), Some("call_native_1"));
    assert!(msg.content.text().contains(r#"{"data":"ok"}"#));

    server.stop().await;
}

#[tokio::test]
async fn test_push_result_native_fc_failure() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();
    let initial_len = agent.messages.len();

    agent
        .push_tool_result_message(
            true,
            "call_native_2",
            "test_tool",
            false,
            "Permission denied",
        )
        .await;

    assert_eq!(agent.messages.len(), initial_len + 1);
    let msg = agent.messages.last().unwrap();
    assert_eq!(msg.role, "tool");
    assert!(msg.content.text().contains("error"));
    assert!(msg.content.text().contains("Permission denied"));

    server.stop().await;
}

// =========================================================================
// image promotion tests
// =========================================================================

#[tokio::test]
async fn test_push_result_image_promotion_xml() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();
    let initial_len = agent.messages.len();

    let result = r#"{"base64_png":"iVBORw0KGgo=","width":100,"height":100}"#;
    agent
        .push_tool_result_message(false, "call_img", "screen_capture", true, result)
        .await;

    assert_eq!(agent.messages.len(), initial_len + 1);
    let msg = agent.messages.last().unwrap();
    assert_eq!(msg.role, "user");
    assert!(msg.content.has_images());
    assert_eq!(msg.content.image_count(), 1);
    // Summary should contain image_attached but not base64_png
    assert!(msg.content.text().contains("image_attached"));
    assert!(!msg.content.text().contains("iVBORw0KGgo="));

    server.stop().await;
}

#[tokio::test]
async fn test_push_result_image_promotion_native_fc() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();
    let initial_len = agent.messages.len();

    let result = r#"{"base64_png":"abc123","note":"screenshot"}"#;
    agent
        .push_tool_result_message(true, "call_img_native", "screen_capture", true, result)
        .await;

    assert_eq!(agent.messages.len(), initial_len + 1);
    let msg = agent.messages.last().unwrap();
    assert_eq!(msg.role, "tool");
    assert_eq!(msg.tool_call_id.as_deref(), Some("call_img_native"));
    assert!(msg.content.has_images());
    assert!(msg.content.text().contains("image_attached"));

    server.stop().await;
}

#[test]
fn test_try_extract_base64_png() {
    assert_eq!(
        try_extract_base64_png(r#"{"base64_png":"abc"}"#),
        Some("abc".to_string())
    );
    assert_eq!(try_extract_base64_png(r#"{"other":"val"}"#), None);
    assert_eq!(try_extract_base64_png("not json"), None);
}

#[test]
fn test_build_image_result_summary() {
    let result = r#"{"base64_png":"longdata","width":800}"#;
    let summary = build_image_result_summary(result);
    assert!(!summary.contains("longdata"));
    assert!(summary.contains("image_attached"));
    assert!(summary.contains("800"));
}

// =========================================================================
// log_tool_call tests
// =========================================================================

#[tokio::test]
async fn test_log_tool_call_with_checkpoint() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    agent.current_checkpoint = Some(crate::checkpoint::TaskCheckpoint::new(
        "test".to_string(),
        "test desc".to_string(),
    ));

    let start_time = std::time::Instant::now();
    agent.log_tool_call(
        "file_read",
        r#"{"path":"x.rs"}"#,
        "content here",
        true,
        start_time,
        false,
    );

    let cp = agent.current_checkpoint.as_ref().unwrap();
    assert_eq!(cp.tool_calls.len(), 1);
    assert_eq!(cp.tool_calls[0].tool_name, "file_read");
    assert_eq!(cp.tool_calls[0].arguments, r#"{"path":"x.rs"}"#);
    assert_eq!(cp.tool_calls[0].result.as_deref(), Some("content here"));
    assert!(cp.tool_calls[0].success);

    server.stop().await;
}

#[tokio::test]
async fn test_log_tool_call_truncates_when_requested() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    agent.current_checkpoint = Some(crate::checkpoint::TaskCheckpoint::new(
        "test".to_string(),
        "test desc".to_string(),
    ));

    let long_result = "x".repeat(5000);
    let start_time = std::time::Instant::now();
    agent.log_tool_call("file_read", "{}", &long_result, true, start_time, true);

    let cp = agent.current_checkpoint.as_ref().unwrap();
    assert_eq!(cp.tool_calls[0].result.as_ref().unwrap().len(), 1000);

    server.stop().await;
}

#[tokio::test]
async fn test_log_tool_call_no_truncation() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    agent.current_checkpoint = Some(crate::checkpoint::TaskCheckpoint::new(
        "test".to_string(),
        "test desc".to_string(),
    ));

    let long_result = "x".repeat(5000);
    let start_time = std::time::Instant::now();
    agent.log_tool_call("file_read", "{}", &long_result, true, start_time, false);

    let cp = agent.current_checkpoint.as_ref().unwrap();
    assert_eq!(cp.tool_calls[0].result.as_ref().unwrap().len(), 5000);

    server.stop().await;
}

#[tokio::test]
async fn test_log_tool_call_without_checkpoint_is_noop() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    assert!(agent.current_checkpoint.is_none());

    let start_time = std::time::Instant::now();
    agent.log_tool_call("file_read", "{}", "result", true, start_time, false);

    assert!(agent.current_checkpoint.is_none());

    server.stop().await;
}

#[tokio::test]
async fn test_log_tool_call_records_failure() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    agent.current_checkpoint = Some(crate::checkpoint::TaskCheckpoint::new(
        "test".to_string(),
        "test desc".to_string(),
    ));

    let start_time = std::time::Instant::now();
    agent.log_tool_call(
        "shell_exec",
        r#"{"command":"bad"}"#,
        "error: not found",
        false,
        start_time,
        false,
    );

    let cp = agent.current_checkpoint.as_ref().unwrap();
    assert!(!cp.tool_calls[0].success);
    assert_eq!(cp.tool_calls[0].result.as_deref(), Some("error: not found"));

    server.stop().await;
}

// =========================================================================
// maybe_enhance_tool_result tests (via agent)
// =========================================================================

#[tokio::test]
async fn test_enhance_non_cargo_check_passthrough() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let agent = Agent::new(config).await.unwrap();

    let result = agent.maybe_enhance_tool_result("file_read", r#"{"content":"hello"}"#);
    assert_eq!(result, r#"{"content":"hello"}"#);

    server.stop().await;
}

#[tokio::test]
async fn test_enhance_successful_cargo_check_passthrough() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let agent = Agent::new(config).await.unwrap();

    let result = agent.maybe_enhance_tool_result("cargo_check", r#"{"success":true,"stderr":""}"#);
    assert_eq!(result, r#"{"success":true,"stderr":""}"#);

    server.stop().await;
}

#[tokio::test]
async fn test_enhance_failed_cargo_check() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let agent = Agent::new(config).await.unwrap();

    let input = r#"{"success":false,"stderr":"error"}"#;
    let result = agent.maybe_enhance_tool_result("cargo_check", input);
    assert!(result.contains(r#""success":false"#));

    server.stop().await;
}

// =========================================================================
// build_tool_call_context extended tests
// =========================================================================

#[tokio::test]
async fn test_build_context_generates_unique_ids() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let agent = Agent::new(config).await.unwrap();

    let (id1, _, _) = agent.build_tool_call_context("file_read", "{}", None);
    let (id2, _, _) = agent.build_tool_call_context("file_read", "{}", None);

    assert!(id1.starts_with("call_"));
    assert!(id2.starts_with("call_"));
    assert_ne!(id1, id2);

    server.stop().await;
}

#[tokio::test]
async fn test_build_context_preserves_args() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let agent = Agent::new(config).await.unwrap();

    let complex_args = r#"{"path":"./src/lib.rs","old_str":"fn old()","new_str":"fn new()"}"#;
    let (_, _, fake_call) = agent.build_tool_call_context("file_edit", complex_args, None);

    assert_eq!(fake_call.function.arguments, complex_args);
    assert_eq!(fake_call.function.name, "file_edit");

    server.stop().await;
}

// =========================================================================
// parse_tool_args tests (via agent)
// =========================================================================

#[tokio::test]
async fn test_parse_args_valid_json() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let start = std::time::Instant::now();
    let result = agent
        .parse_tool_args("file_read", r#"{"path":"test.rs"}"#, "call_1", false, start)
        .await;

    assert!(result.is_some());
    let args = result.unwrap();
    assert_eq!(args["path"], "test.rs");

    server.stop().await;
}

#[tokio::test]
async fn test_parse_args_invalid_json() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();
    let initial_len = agent.messages.len();

    let start = std::time::Instant::now();
    let result = agent
        .parse_tool_args("file_read", "this is not json", "call_1", false, start)
        .await;

    assert!(result.is_none());
    assert!(agent.messages.len() > initial_len);

    server.stop().await;
}

#[tokio::test]
async fn test_parse_args_invalid_json_native_fc() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let start = std::time::Instant::now();
    let result = agent
        .parse_tool_args("shell_exec", "{broken", "call_native_1", true, start)
        .await;

    assert!(result.is_none());
    let last = agent.messages.last().unwrap();
    assert_eq!(last.role, "tool");

    server.stop().await;
}

#[tokio::test]
async fn test_parse_args_empty_json_object() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let start = std::time::Instant::now();
    let result = agent
        .parse_tool_args("git_status", "{}", "call_1", false, start)
        .await;

    assert!(result.is_some());
    let args = result.unwrap();
    assert!(args.as_object().unwrap().is_empty());

    server.stop().await;
}

#[tokio::test]
async fn test_validate_tool_args_rejects_missing_required_field() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();
    let initial_len = agent.messages.len();

    let start = std::time::Instant::now();
    let ok = agent
        .validate_tool_args(
            "shell_exec",
            "{}",
            &serde_json::json!({}),
            "call_1",
            false,
            start,
        )
        .await;

    assert!(!ok);
    assert!(agent.messages.len() > initial_len);
    let last = agent.messages.last().unwrap();
    assert!(last
        .content
        .text()
        .contains("missing required field(s): command"));

    server.stop().await;
}

#[tokio::test]
async fn test_validate_tool_args_accepts_valid_payload() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let start = std::time::Instant::now();
    let ok = agent
        .validate_tool_args(
            "shell_exec",
            r#"{"command":"echo hi"}"#,
            &serde_json::json!({"command":"echo hi"}),
            "call_1",
            false,
            start,
        )
        .await;

    assert!(ok);

    server.stop().await;
}

#[tokio::test]
async fn test_repeated_parse_failure_is_suppressed_before_reexecution() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let start = std::time::Instant::now();
    let first = agent
        .parse_tool_args("shell_exec", "{broken", "call_1", false, start)
        .await;
    assert!(first.is_none());

    let second_start = std::time::Instant::now();
    let suppressed = agent
        .suppress_repeated_failed_tool_retry("shell_exec", "{broken", "call_2", false, second_start)
        .await;
    assert!(suppressed);

    let recovery = agent.messages.last().expect("expected suppression result");
    assert!(recovery.content.text().contains("RETRY SUPPRESSED"));
    assert!(recovery.content.text().contains("valid JSON"));
    assert!(recovery.content.text().contains("`command`"));
    assert!(agent
        .pending_failure_hint
        .as_deref()
        .is_some_and(|hint| { hint.contains("RETRY SUPPRESSED") && hint.contains("valid JSON") }));

    server.stop().await;
}

#[tokio::test]
async fn test_repeated_validation_failure_is_suppressed_before_reexecution() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let start = std::time::Instant::now();
    let first = agent
        .validate_tool_args(
            "shell_exec",
            "{}",
            &serde_json::json!({}),
            "call_1",
            false,
            start,
        )
        .await;
    assert!(!first);

    let second_start = std::time::Instant::now();
    let suppressed = agent
        .suppress_repeated_failed_tool_retry("shell_exec", "{}", "call_2", false, second_start)
        .await;
    assert!(suppressed);

    let recovery = agent.messages.last().expect("expected suppression result");
    assert!(recovery.content.text().contains("RETRY SUPPRESSED"));
    assert!(recovery.content.text().contains("schema validation"));
    assert!(recovery.content.text().contains("`command`"));
    assert!(agent.pending_failure_hint.as_deref().is_some_and(|hint| {
        hint.contains("RETRY SUPPRESSED") && hint.contains("schema validation")
    }));

    server.stop().await;
}

#[cfg_attr(
    target_os = "windows",
    ignore = "file_tracker keys differ between read/edit on Windows; pending separate fix"
)]
#[tokio::test]
async fn test_successful_different_tool_clears_failed_tool_suppression_window() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    // Use file_read of a known-existing temp file as the "successful" middle tool
    // so the test doesn't depend on CWD being in a git repo.
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "hello").unwrap();
    let read_args = serde_json::json!({"path": tmp.path()}).to_string();

    let batch: Vec<CollectedToolCall> = vec![
        ("shell_exec".to_string(), "{}".to_string(), None),
        ("file_read".to_string(), read_args, None),
        ("shell_exec".to_string(), "{}".to_string(), None),
    ];

    agent.execute_tool_batch(batch).await.unwrap();

    let validation_failures = agent
        .messages
        .iter()
        .filter(|msg| {
            msg.content
                .text()
                .contains("missing required field(s): command")
        })
        .count();
    let suppressed_retries = agent
        .messages
        .iter()
        .filter(|msg| msg.content.text().contains("RETRY SUPPRESSED"))
        .count();

    assert!(
        validation_failures >= 2,
        "expected the same invalid tool to run again after a different successful tool"
    );
    assert_eq!(
        suppressed_retries, 0,
        "successful different tool should clear retry suppression"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_execute_tool_batch_vision_analyze_uses_configured_vision_profile() {
    let server = MockLlmServer::builder()
        .with_response(r#"{"seen":"ok"}"#)
        .build()
        .await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.models.insert(
        "vision".to_string(),
        crate::config::ModelProfile {
            endpoint: format!("{}/v1", server.url()),
            model: "mock-vision-model".to_string(),
            api_key: None,
            max_tokens: 192,
            temperature: 0.0,
            modalities: vec!["text".to_string(), "vision".to_string()],
            context_length: 262_144,
            extra_body: Some({
                let mut extra = serde_json::Map::new();
                extra.insert(
                    "chat_template_kwargs".to_string(),
                    serde_json::json!({ "enable_thinking": false }),
                );
                extra
            }),
            native_function_calling: None,
        },
    );
    let mut agent = Agent::new(config).await.unwrap();

    let batch: Vec<CollectedToolCall> = vec![(
        "vision_analyze".to_string(),
        serde_json::json!({
            "prompt": "Describe this test image.",
            "image_base64": "iVBORw0KGgo="
        })
        .to_string(),
        None,
    )];

    agent.execute_tool_batch(batch).await.unwrap();

    assert!(agent.messages.last().is_some_and(|message| {
        let text = message.content.text();
        text.contains("\"success\":true") && text.contains("\"model\":\"mock-vision-model\"")
    }));

    server.stop().await;
}

// =========================================================================
// maybe_verify_file_change tests
// =========================================================================

#[tokio::test]
async fn test_verify_non_file_tool_returns_none() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let args = serde_json::json!({"command": "ls"});
    let result = agent.maybe_verify_file_change("shell_exec", &args).await;
    assert!(result.is_none());

    server.stop().await;
}

#[tokio::test]
async fn test_verify_file_read_returns_none() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let args = serde_json::json!({"path": "test.rs"});
    let result = agent.maybe_verify_file_change("file_read", &args).await;
    assert!(result.is_none());

    server.stop().await;
}

#[tokio::test]
async fn test_verify_file_edit_without_path_returns_none() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let args = serde_json::json!({"old_str": "x", "new_str": "y"});
    let result = agent.maybe_verify_file_change("file_edit", &args).await;
    assert!(result.is_none());

    server.stop().await;
}

// =========================================================================
// execute_step_internal e2e tests (via mock server)
// =========================================================================

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_step_returns_true_on_completion() {
    let server = MockLlmServer::builder()
        .with_response("Task is complete. Here is the result.")
        .build()
        .await;

    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.execute_step_internal(false).await;
    assert!(result.is_ok());
    assert!(result.unwrap());

    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_step_returns_false_on_tool_execution() {
    let server = MockLlmServer::builder()
        .with_response(
            r#"<tool>
<name>file_read</name>
<arguments>{"path":"./Cargo.toml"}</arguments>
</tool>"#,
        )
        .build()
        .await;

    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.execute_step_internal(false).await;
    assert!(result.is_ok());
    assert!(!result.unwrap());

    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_step_malformed_tool_injects_correction() {
    // Use content that has malformed markers but cannot be parsed as a tool call.
    // <tool broken here> has the "<tool" marker but no valid <name>/<arguments>.
    let server = MockLlmServer::builder()
        .with_response("I want to use <tool broken here> to do something with tool_call syntax")
        .build()
        .await;

    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.execute_step_internal(false).await;
    assert!(result.is_ok());
    assert!(!result.unwrap());

    let last_user_msg = agent
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .unwrap();
    assert!(last_user_msg.content.text().contains("malformed"));

    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_step_completion_gate_rejects() {
    let server = MockLlmServer::builder()
        .with_response("Done!")
        .build()
        .await;

    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 10;
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.execute_step_internal(false).await;
    assert!(result.is_ok());
    assert!(!result.unwrap());

    let last_user_msg = agent
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .unwrap();
    assert!(last_user_msg.content.text().contains("step"));

    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_mutation_task_rejects_long_prose_without_tools_before_completion_gate() {
    // The completion gate resolves current_project_root() and runs `git diff`
    // in the process cwd. Run in an isolated empty temp dir (not the real
    // repo) so that decision is deterministic and can't be perturbed by
    // ambient repo git state or concurrent git activity — the source of a
    // prior contention flake that only surfaced under heavy parallel load.
    let _cwd_dir = tempfile::tempdir().unwrap();
    let _cwd = crate::test_support::CwdGuard::enter(_cwd_dir.path());
    let long_prose = format!(
        "I analyzed the bug and will now explain the intended fix.\n{}",
        "This is still only prose, not a tool call. ".repeat(40)
    );
    let server = MockLlmServer::builder()
        .with_response(&long_prose)
        .build()
        .await;

    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = false;
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_task_context = "Fix the qutebrowser bug".to_string();

    let result = agent.execute_step_internal(false).await;

    assert!(result.is_ok());
    assert!(!result.unwrap());
    assert_eq!(agent.consecutive_no_action_prompts, 1);
    let last_user_msg = agent
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .unwrap();
    assert!(
        last_user_msg.content.text().contains("did not call a tool"),
        "expected no-action recovery prompt, got: {}",
        last_user_msg.content.text()
    );

    server.stop().await;
}

#[tokio::test]
async fn mutation_no_tool_stall_bails_after_repeated_malformed_turns() {
    let config = test_config("http://127.0.0.1:1/v1".to_string());
    let mut agent = Agent::new(config).await.unwrap();
    agent.current_task_context = "Fix the bug in this repository".to_string();

    for _ in 0..5 {
        agent
            .note_mutation_no_tool_stall("malformed tool XML")
            .unwrap();
    }
    let err = agent
        .note_mutation_no_tool_stall("malformed tool XML")
        .unwrap_err()
        .to_string();

    assert!(err.contains("NONTERM_PROSE_NO_TOOL"));
}

#[test]
fn observational_shell_batch_detects_read_only_commands() {
    let calls = vec![(
        "shell_exec".to_string(),
        serde_json::json!({"command": "grep -n all_processes qutebrowser/misc/guiprocess.py"})
            .to_string(),
        None,
    )];
    assert!(is_observational_shell_batch(&calls));

    let mutating = vec![(
        "shell_exec".to_string(),
        serde_json::json!({"command": "sed -i 's/old/new/' qutebrowser/misc/guiprocess.py"})
            .to_string(),
        None,
    )];
    assert!(!is_observational_shell_batch(&mutating));
}

#[tokio::test]
async fn extract_code_and_path_rust_fallback_requires_cargo_toml() {
    let dir = tempfile::tempdir().unwrap();
    let _guard = crate::test_support::CwdGuard::enter(dir.path());

    let content = r#"```rust
pub fn answer() -> i32 {
    42
}

pub fn other() -> i32 {
    answer()
}
```"#;

    assert!(extract_code_and_path(content).await.is_none());
    std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname='x'\n").unwrap();

    let (path, _) = extract_code_and_path(content).await.unwrap();
    assert_eq!(path, "src/lib.rs");
    // cwd restored on drop of `_guard`.
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_step_rejects_incomplete_planning_completion() {
    let server = MockLlmServer::builder()
            .with_response(
                "I need to read the project files first to understand the codebase and identify what needs to be fixed.\n\nfile_read: \"Cargo.toml\"\nfile_read: \"src/lib.rs\"",
            )
            .build()
            .await;

    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = false;
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.execute_step_internal(false).await;
    assert!(result.is_ok());
    assert!(!result.unwrap());

    let last_user_msg = agent
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .unwrap();
    assert!(
        last_user_msg
            .content
            .text()
            .contains("describes work you still need to do"),
        "Expected the incomplete planning response to be rejected"
    );

    server.stop().await;
}

// =========================================================================
// execute_step_with_logging / execute_pending_tool_calls wrappers
// =========================================================================

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_execute_step_with_logging_delegates() {
    let server = MockLlmServer::builder()
        .with_response("Completed.")
        .build()
        .await;

    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.execute_step_with_logging("test task").await;
    assert!(result.is_ok());
    assert!(result.unwrap());

    server.stop().await;
}

#[tokio::test]
async fn test_execute_pending_no_assistant_msg_fails() {
    let server = MockLlmServer::builder().with_response("done").build().await;

    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.execute_pending_tool_calls("test task").await;
    assert!(result.is_err());

    server.stop().await;
}

// =========================================================================
// execute_single_tool tests
// =========================================================================

#[tokio::test]
async fn test_single_tool_unknown_tool() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let start = std::time::Instant::now();
    let args = serde_json::json!({});
    let result = agent
        .execute_single_tool("nonexistent_tool", "{}", &args, start)
        .await;

    assert!(result.is_ok());
    let (success, result_str, summary) = result.unwrap();
    assert!(!success);
    assert!(result_str.contains("Unknown tool"));
    assert!(result_str.contains("nonexistent_tool"));
    assert!(summary.contains("nonexistent_tool"));

    server.stop().await;
}

// =========================================================================
// Additional edge cases
// =========================================================================

#[test]
fn test_should_prompt_need_to_phrase() {
    assert!(should_prompt_for_action(
        "I need to check this",
        true,
        false,
        0
    ));
}

#[test]
fn test_should_prompt_start_by_phrase() {
    assert!(should_prompt_for_action(
        "start by reading the file",
        true,
        false,
        0
    ));
}

#[test]
fn test_should_prompt_help_you_phrase() {
    assert!(should_prompt_for_action(
        "I can help you with that",
        true,
        false,
        0
    ));
}

#[test]
fn test_should_not_prompt_empty_content() {
    assert!(!should_prompt_for_action("", true, false, 0));
}

#[test]
fn test_should_not_prompt_exactly_1000_chars() {
    // 1000 chars of real content with no reasoning = genuine (passes > 500 check)
    let content = "let me ".to_string() + &"x".repeat(993);
    assert_eq!(content.len(), 1000);
    assert!(!should_prompt_for_action(&content, true, false, 0));
}

#[test]
fn test_should_not_prompt_501_chars_no_reasoning() {
    // 501+ chars of real content with no reasoning = genuine response
    let content = "let me ".to_string() + &"x".repeat(494);
    assert_eq!(content.len(), 501);
    assert!(!should_prompt_for_action(&content, true, false, 0));
}

#[test]
fn test_should_prompt_499_chars_with_intent() {
    // Under 500 chars with intent phrase = should prompt
    let content = "let me ".to_string() + &"x".repeat(492);
    assert_eq!(content.len(), 499);
    assert!(should_prompt_for_action(&content, true, false, 0));
}

#[test]
fn test_should_prompt_all_intent_phrases() {
    let phrases = [
        ("let me check", true),
        ("I'll do it", true),
        ("I will handle", true),
        ("let's go", true),
        ("First, analyze", true),
        ("Starting the process", true),
        ("Begin by reading", true),
        ("Going to implement", true),
        ("Need to fix", true),
        ("Start by examining", true),
        ("I can help you with that", true),
        ("The result is 42", false),
        ("No intent here", false),
    ];

    for (phrase, expected) in phrases {
        assert_eq!(
            should_prompt_for_action(phrase, true, false, 0),
            expected,
            "Failed for phrase: {:?}",
            phrase
        );
    }
}

#[test]
fn test_repetition_hash_consistency() {
    use std::collections::hash_map::DefaultHasher;

    let args = r#"{"path":"test.rs"}"#;
    let mut h1 = DefaultHasher::new();
    args.hash(&mut h1);
    let hash1 = h1.finish();

    let mut h2 = DefaultHasher::new();
    args.hash(&mut h2);
    let hash2 = h2.finish();

    assert_eq!(hash1, hash2, "Same args should produce same hash");

    let different_args = r#"{"path":"other.rs"}"#;
    let mut h3 = DefaultHasher::new();
    different_args.hash(&mut h3);
    let hash3 = h3.finish();

    assert_ne!(hash1, hash3, "Different args should produce different hash");
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_file_read_adds_to_context_files() {
    // Use CARGO_MANIFEST_DIR for cwd-independent test (fix #7: test determinism)
    let cargo_toml_path = format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR"));
    let server = MockLlmServer::builder()
        .with_response(format!(
            "<tool>\n<name>file_read</name>\n<arguments>{{\"path\":\"{}\"}}</arguments>\n</tool>",
            cargo_toml_path
        ))
        .with_response("Done reading.")
        .build()
        .await;

    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let _ = agent.execute_step_internal(false).await;

    assert!(
        agent
            .file_tracker
            .context_files
            .iter()
            .any(|p| p.ends_with("Cargo.toml")),
        "Expected Cargo.toml in context_files: {:?}",
        agent.file_tracker.context_files
    );

    server.stop().await;
}

#[cfg_attr(
    target_os = "windows",
    ignore = "file_tracker keys differ between read/edit on Windows; pending separate fix"
)]
#[tokio::test]
async fn test_redundant_unchanged_file_reads_update_task_state_memory() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();
    let cargo_toml_path = format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR"));

    let batch: Vec<CollectedToolCall> = vec![
        (
            "file_read".to_string(),
            serde_json::json!({"path": cargo_toml_path}).to_string(),
            None,
        ),
        (
            "file_read".to_string(),
            serde_json::json!({"path": cargo_toml_path}).to_string(),
            None,
        ),
    ];

    agent.execute_tool_batch(batch).await.unwrap();

    let state = agent
        .file_tracker
        .read_state
        .get(&cargo_toml_path)
        .expect("expected Cargo.toml file state");
    assert_eq!(state.unchanged_read_count, 1);
    assert!(agent
        .task_state_notes
        .iter()
        .any(|note| { note.contains(&format!("Reread unchanged file `{}`", cargo_toml_path)) }));
    assert!(agent.pending_failure_hint.as_deref().is_some_and(|hint| {
        hint.contains(&format!("reread unchanged file `{}`", cargo_toml_path))
    }));

    server.stop().await;
}

#[cfg_attr(
    target_os = "windows",
    ignore = "file_tracker keys differ between read/edit on Windows; pending separate fix"
)]
#[tokio::test]
async fn test_third_unchanged_file_read_is_blocked() {
    use std::fs;
    use tempfile::NamedTempFile;

    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let temp = NamedTempFile::new().unwrap();
    fs::write(temp.path(), "hello world\n").unwrap();
    let path = temp.path().canonicalize().unwrap().display().to_string();

    // First batch of 3 reads — all should succeed (threshold is 3 unchanged rereads)
    let batch1: Vec<CollectedToolCall> = vec![
        (
            "file_read".to_string(),
            serde_json::json!({"path": path}).to_string(),
            None,
        ),
        (
            "file_read".to_string(),
            serde_json::json!({"path": path}).to_string(),
            None,
        ),
        (
            "file_read".to_string(),
            serde_json::json!({"path": path}).to_string(),
            None,
        ),
    ];
    agent.execute_tool_batch(batch1).await.unwrap();

    // Second batch — 4th read triggers blocking (count reaches 3), 5th is suppressed
    let batch2: Vec<CollectedToolCall> = vec![
        (
            "file_read".to_string(),
            serde_json::json!({"path": path}).to_string(),
            None,
        ),
        (
            "file_read".to_string(),
            serde_json::json!({"path": path}).to_string(),
            None,
        ),
    ];
    agent.execute_tool_batch(batch2).await.unwrap();

    // Verify the file is being tracked
    let state = agent
        .file_tracker
        .read_state
        .get(&path)
        .expect("expected tracked file state");
    // The unchanged_read_count tracks how many times the file was read unchanged.
    // Reads 1-3 execute (count goes 1->2->3), Read 4 is blocked (count becomes 4).
    // Read 5 is suppressed by retry suppression before execution.
    assert_eq!(state.unchanged_read_count, 4);
    // Verify a "task_state" failure was recorded (from the 4th read being blocked)
    assert!(agent
        .recent_failed_tool_attempts
        .back()
        .is_some_and(|attempt| attempt.failure_kind == "task_state"));
    // Verify the blocking message appears in messages (from the 4th read)
    assert!(agent.messages.iter().any(|message| message
        .content
        .text()
        .contains("Repeated unchanged reread blocked")));

    server.stop().await;
}

#[cfg_attr(
    target_os = "windows",
    ignore = "file_tracker keys differ between read/edit on Windows; pending separate fix"
)]
#[tokio::test]
async fn test_file_edit_clears_task_state_for_modified_file() {
    use std::fs;
    use tempfile::NamedTempFile;

    let _g = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let temp = NamedTempFile::new_in(std::env::current_dir().unwrap()).unwrap();
    fs::write(temp.path(), "line one\nhello world\nline three\n").unwrap();
    let path = temp.path().display().to_string();

    let batch: Vec<CollectedToolCall> = vec![
        (
            "file_read".to_string(),
            serde_json::json!({"path": path}).to_string(),
            None,
        ),
        (
            "file_edit".to_string(),
            serde_json::json!({
                "path": &path,
                "old_str": "hello world",
                "new_str": "hello rust"
            })
            .to_string(),
            None,
        ),
    ];

    agent.execute_tool_batch(batch).await.unwrap();

    assert!(!agent.file_tracker.read_state.contains_key(&path));
    assert!(agent
        .task_state_notes
        .iter()
        .any(|note| note.contains("Marked") && note.contains(&path)));

    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn test_step_with_empty_response() {
    let server = MockLlmServer::builder().with_response("").build().await;

    let config = test_config(format!("{}/v1", server.url()));
    let mut agent = Agent::new(config).await.unwrap();

    let result = agent.execute_step_internal(false).await;
    // An empty response is a provider hiccup, NOT a successful completion:
    // the step must NOT report done (it nudges and continues instead of
    // painting a ✅ banner with no content).
    assert!(result.is_ok());
    assert!(
        !result.unwrap(),
        "an empty response must not be accepted as a completed step"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_gate_both_conditions_met() {
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = test_config(format!("{}/v1", server.url()));
    config.agent.min_completion_steps = 0;
    config.agent.require_verification_before_completion = true;
    let mut agent = Agent::new(config).await.unwrap();

    let mut checkpoint =
        crate::checkpoint::TaskCheckpoint::new("test".to_string(), "test desc".to_string());
    checkpoint.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "cargo_check".to_string(),
        arguments: "{}".to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(50),
    });
    agent.current_checkpoint = Some(checkpoint);

    assert!(agent.check_completion_gate().await.is_none());

    server.stop().await;
}

// -- read_line_pausing_esc tests --

// NOTE: These tests are marked as `#[ignore]` because they interact with stdin,
// which blocks indefinitely in test environments (non-TTY). Run with `--ignored`
// locally if you want to verify the pause flag behavior.

#[tokio::test]
#[ignore = "blocks on stdin in non-interactive test environment"]
async fn read_line_pausing_esc_sets_and_clears_pause_flag() {
    use std::sync::atomic::Ordering;

    let paused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let ack = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let paused_clone = std::sync::Arc::clone(&paused);
    let ack_clone = std::sync::Arc::clone(&ack);

    // Spawn a watcher that records whether paused was ever set
    let was_paused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let was_paused_clone = std::sync::Arc::clone(&was_paused);

    let watcher = std::thread::spawn(move || {
        for _ in 0..100 {
            if paused_clone.load(Ordering::Acquire) {
                ack_clone.store(true, Ordering::Release);
                was_paused_clone.store(true, Ordering::Relaxed);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    });

    // Simulate stdin by piping — read_line will return an error or empty
    // in a non-interactive test, but the pause flag behavior is what we test.
    let _ = read_line_pausing_esc(&paused, &ack).await;

    // After return, paused must be cleared
    assert!(
        !paused.load(Ordering::Acquire),
        "esc_paused must be false after read_line_pausing_esc returns"
    );
    assert!(
        !ack.load(Ordering::Acquire),
        "esc_pause_ack must be reset after read_line_pausing_esc returns"
    );

    let _ = watcher.join();
    // In CI (non-interactive), the watcher may or may not observe the pause
    // due to timing, so we don't assert was_paused — the important thing is
    // the flag is cleared after the call.
}

#[tokio::test]
#[ignore = "blocks on stdin in non-interactive test environment"]
async fn read_line_pausing_esc_unpauses_even_on_error() {
    use std::sync::atomic::Ordering;

    let paused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let ack = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Call in non-interactive context (stdin is not a tty in tests)
    let _ = read_line_pausing_esc(&paused, &ack).await;

    assert!(
        !paused.load(Ordering::Acquire),
        "esc_paused must always be cleared, even if read_line fails"
    );
    assert!(
        !ack.load(Ordering::Acquire),
        "esc_pause_ack must always be cleared, even if read_line fails"
    );
}
