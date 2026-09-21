use super::*;
use std::time::Duration;

// ── format_file_size tests ──

#[test]
fn format_file_size_bytes() {
    assert_eq!(Agent::format_file_size(0), "0B");
    assert_eq!(Agent::format_file_size(512), "512B");
    assert_eq!(Agent::format_file_size(1023), "1023B");
}

#[test]
fn format_file_size_kilobytes() {
    assert_eq!(Agent::format_file_size(1024), "1.0KB");
    assert_eq!(Agent::format_file_size(2048), "2.0KB");
    assert_eq!(Agent::format_file_size(1536), "1.5KB");
}

#[test]
fn format_file_size_megabytes() {
    assert_eq!(Agent::format_file_size(1024 * 1024), "1.0MB");
    assert_eq!(Agent::format_file_size(2 * 1024 * 1024), "2.0MB");
}

// ── Slash command matching patterns ──
// These tests verify the string-matching logic used in the interactive loop
// to route slash commands, extracted as pure assertions.

#[test]
fn slash_command_routing_exact_matches() {
    let commands = vec![
        "/help",
        "/status",
        "/stats",
        "/compress",
        "/clear",
        "/tools",
        "/mode",
        "/ctx",
        "/context",
        "/diff",
        "/git",
        "/undo",
        "/cost",
        "/model",
        "/last",
        "/debug",
        "/debug-log",
        "/compact",
        "/verbose",
        "/config",
        "/memory",
        "/copy",
        "/restore",
        "/vim",
        "/theme",
        "/queue",
        "/swarm",
        "/chat",
    ];
    for cmd in &commands {
        assert!(
            cmd.starts_with('/'),
            "Command '{}' should start with /",
            cmd
        );
    }

    // Non-slash input should NOT be treated as a command
    let non_commands = ["help", "status", "hello", "fix the bug"];
    for input in &non_commands {
        assert!(
            !input.starts_with('/'),
            "'{}' should not be treated as a slash command",
            input
        );
    }
}

#[test]
fn looks_like_slash_command_matches_unhandled_commands() {
    // Registry-advertised commands with no REPL handler must be caught
    // by the unknown-slash guard instead of burning a paid chat message.
    for input in [
        "/mode yolo",
        "/analyze",
        "/analyze ./src",
        "/garden",
        "/journal",
        "/palette",
        "/totally-made-up",
        "/help extra args",
    ] {
        assert!(
            looks_like_slash_command(input),
            "'{}' should be treated as a slash command",
            input
        );
    }
}

#[test]
fn looks_like_slash_command_passes_paths_and_chat() {
    // Absolute paths and ordinary chat must still reach the LLM.
    for input in [
        "/tmp/foo.rs",
        "/home/rig/selfware/src/main.rs",
        "/",
        "/..",
        "hello",
        "fix the /bug in /src/main.rs",
        "",
    ] {
        assert!(
            !looks_like_slash_command(input),
            "'{}' should NOT be treated as a slash command",
            input
        );
    }
}

#[test]
fn slash_command_with_argument_parsing() {
    // Verify strip_prefix patterns used throughout the interactive loop
    let input = "/review src/main.rs";
    let arg = input.strip_prefix("/review ").map(str::trim);
    assert_eq!(arg, Some("src/main.rs"));

    let input = "/analyze ./src";
    let arg = input.strip_prefix("/analyze ").map(str::trim);
    assert_eq!(arg, Some("./src"));

    let input = "/plan implement auth flow";
    let arg = input.strip_prefix("/plan ").map(str::trim);
    assert_eq!(arg, Some("implement auth flow"));

    let input = "/swarm refactor error handling";
    let arg = input.strip_prefix("/swarm ").map(str::trim);
    assert_eq!(arg, Some("refactor error handling"));

    let input = "/queue fix the tests";
    let arg = input.strip_prefix("/queue ").map(str::trim);
    assert_eq!(arg, Some("fix the tests"));
}

#[test]
fn context_command_aliases() {
    // Both /context and /ctx should work for all subcommands
    let aliases = [("/context", "/ctx"), ("/context clear", "/ctx clear")];
    for (full, short) in &aliases {
        assert!(full.starts_with("/context") || full.starts_with("/ctx"));
        assert!(short.starts_with("/ctx"));
    }

    let load_input = "/ctx load .rs,.toml";
    let arg = load_input
        .strip_prefix("/context load ")
        .or_else(|| load_input.strip_prefix("/ctx load "))
        .map(str::trim);
    assert_eq!(arg, Some(".rs,.toml"));
}

// ── Shell escape parsing ──

#[test]
fn shell_escape_command_extraction() {
    // The interactive loop uses `!` prefix for shell escapes
    let input = "!ls -la";
    assert!(input.starts_with('!'));
    let cmd = input.strip_prefix('!').map(str::trim);
    assert_eq!(cmd, Some("ls -la"));

    let input = "! git status";
    let cmd = input.strip_prefix('!').map(str::trim);
    assert_eq!(cmd, Some("git status"));

    // Empty shell command
    let input = "!";
    let cmd = input.strip_prefix('!').map(str::trim);
    assert_eq!(cmd, Some(""));
}

// ── Exit/quit detection ──

#[test]
fn exit_commands_recognized() {
    for input in &["exit", "quit", "/exit", "/quit", "/q"] {
        assert!(is_exit_command(input), "'{}' should trigger exit", input);
    }

    for input in &[
        "exiting",
        "quitting",
        "EXIT",
        "exit now",
        "query",
        "/question",
        "q",
    ] {
        assert!(
            !is_exit_command(input),
            "'{}' should NOT trigger exit",
            input
        );
    }
}

// ── Large paste preview logic ──

#[test]
fn large_paste_detection() {
    const LARGE_PASTE_THRESHOLD: usize = 3000;
    const PREVIEW_CHARS: usize = 200;

    let small_input = "Hello world";
    assert!(small_input.len() <= LARGE_PASTE_THRESHOLD);

    let large_input = "x".repeat(5000);
    assert!(large_input.len() > LARGE_PASTE_THRESHOLD);

    // Verify preview extraction logic
    let start_preview: String = large_input.chars().take(PREVIEW_CHARS).collect();
    assert_eq!(start_preview.len(), PREVIEW_CHARS);

    let end_preview: String = large_input
        .chars()
        .rev()
        .take(PREVIEW_CHARS)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    assert_eq!(end_preview.len(), PREVIEW_CHARS);
}

// ── Queued message preview truncation ──

#[test]
fn queued_message_preview_truncation() {
    let short_msg = "Short message";
    let preview = preview_with_ellipsis(short_msg, QUEUE_DRAIN_PREVIEW_BYTES);
    assert_eq!(preview, "Short message");

    let long_msg = "a".repeat(200);
    let preview = preview_with_ellipsis(&long_msg, QUEUE_DRAIN_PREVIEW_BYTES);
    assert!(preview.len() <= QUEUE_DRAIN_PREVIEW_BYTES + 3);
    assert!(preview.ends_with("..."));
}

#[test]
fn strip_trailing_submission_newlines_preserves_multiline_content() {
    let pasted = "def chart():\n    return 42\n\n";
    assert_eq!(
        strip_trailing_submission_newlines(pasted),
        "def chart():\n    return 42"
    );

    let carriage_return = "line one\r\nline two\r\n";
    assert_eq!(
        strip_trailing_submission_newlines(carriage_return),
        "line one\r\nline two"
    );
}

// ── Queue management subcommand routing ──

#[test]
fn queue_subcommand_routing() {
    // /queue list and /queue clear must match before /queue <msg>
    let input = "/queue list";
    assert!(input == "/queue list");
    assert!(input.starts_with("/queue ")); // would also match generic handler

    let input = "/queue clear";
    assert!(input == "/queue clear");
    assert!(input.starts_with("/queue ")); // would also match generic handler

    // /queue drop <n> uses strip_prefix
    let input = "/queue drop 3";
    let idx_str = input.strip_prefix("/queue drop ");
    assert_eq!(idx_str, Some("3"));
    let idx: usize = idx_str.unwrap().trim().parse().unwrap();
    assert_eq!(idx, 3);

    // /queue drop with extra whitespace
    let input = "/queue drop  5 ";
    let idx_str = input.strip_prefix("/queue drop ");
    assert_eq!(idx_str.unwrap().trim().parse::<usize>().unwrap(), 5);

    // /queue drop with invalid index
    let input = "/queue drop abc";
    let idx_str = input.strip_prefix("/queue drop ").unwrap();
    assert!(idx_str.trim().parse::<usize>().is_err());
}

#[test]
fn queue_subcommands_do_not_match_bare_queue() {
    // /queue (bare) should not match subcommands
    let input = "/queue";
    assert!(input == "/queue");
    assert!(!input.starts_with("/queue ")); // no trailing space
}

#[test]
fn queue_drop_index_conversion() {
    // 1-based to 0-based conversion via saturating_sub
    assert_eq!(1_usize.saturating_sub(1), 0);
    assert_eq!(5_usize.saturating_sub(1), 4);
    // Edge case: 0 stays at 0 (saturating)
    assert_eq!(0_usize.saturating_sub(1), 0);
}

#[test]
fn queue_list_preview_truncation() {
    let short = "Short message";
    let preview = preview_with_ellipsis(short, QUEUE_LIST_PREVIEW_BYTES);
    assert_eq!(preview, "Short message");

    let long = "x".repeat(200);
    let preview = preview_with_ellipsis(&long, QUEUE_LIST_PREVIEW_BYTES);
    assert!(preview.len() <= QUEUE_LIST_PREVIEW_BYTES + 3);
    assert!(preview.ends_with("..."));

    let emoji_str = "Hello 🦊 world! This is a test with emoji 🌸 and more text here...";
    let preview = preview_with_ellipsis(emoji_str, QUEUE_LIST_PREVIEW_BYTES);
    assert!(preview.len() <= QUEUE_LIST_PREVIEW_BYTES + 3);
}

#[test]
fn queue_drop_preview_truncation() {
    let short = "Short task";
    let preview = preview_with_ellipsis(short, QUEUE_DROP_PREVIEW_BYTES);
    assert_eq!(preview, "Short task");

    let long = "y".repeat(120);
    let preview = preview_with_ellipsis(&long, QUEUE_DROP_PREVIEW_BYTES);
    assert!(preview.len() <= QUEUE_DROP_PREVIEW_BYTES + 3);
    assert!(preview.ends_with("..."));

    let emoji_str = "🦊🌸🌿❄️🥀 abcdefghij 🦊🌸🌿❄️🥀";
    let preview = preview_with_ellipsis(emoji_str, QUEUE_DROP_PREVIEW_BYTES);
    assert!(preview.len() <= QUEUE_DROP_PREVIEW_BYTES + 3);
}

#[test]
fn coalesces_interactive_queue_bursts_into_one_message() {
    let start = Instant::now();
    let messages = vec![
        PendingMessage::new("line one", PendingMessageOrigin::InteractiveQueue, start),
        PendingMessage::new(
            "line two",
            PendingMessageOrigin::InteractiveQueue,
            start + Duration::from_millis(25),
        ),
        PendingMessage::new(
            "manual follow-up",
            PendingMessageOrigin::ManualQueue,
            start + Duration::from_millis(30),
        ),
    ];

    let coalesced = coalesce_pending_messages(messages);
    assert_eq!(coalesced.len(), 2);
    assert_eq!(coalesced[0].content, "line one\nline two");
    assert_eq!(coalesced[1].content, "manual follow-up");
}

#[test]
fn queue_vecdeque_operations() {
    use std::collections::VecDeque;

    let now = Instant::now();
    let mut queue: VecDeque<PendingMessage> = VecDeque::new();

    queue.push_back(PendingMessage::new(
        "task one",
        PendingMessageOrigin::ManualQueue,
        now,
    ));
    queue.push_back(PendingMessage::new(
        "task two",
        PendingMessageOrigin::ManualQueue,
        now,
    ));
    queue.push_back(PendingMessage::new(
        "task three",
        PendingMessageOrigin::ManualQueue,
        now,
    ));
    assert_eq!(queue.len(), 3);

    let items: Vec<(usize, &PendingMessage)> = queue.iter().enumerate().collect();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].0, 0);
    assert_eq!(items[0].1.content, "task one");

    let removed = queue.remove(1).unwrap();
    assert_eq!(removed.content, "task two");
    assert_eq!(queue.len(), 2);
    assert_eq!(queue[0].content, "task one");
    assert_eq!(queue[1].content, "task three");

    // Clear
    let count = queue.len();
    queue.clear();
    assert_eq!(count, 2);
    assert!(queue.is_empty());
}

// ── ESC listener pause/unpause tests ──

#[tokio::test]
async fn esc_listener_stops_cleanly() {
    let cancel = Arc::new(AtomicBool::new(false));
    let paused = Arc::new(AtomicBool::new(false));
    let ack = Arc::new(AtomicBool::new(false));
    let guard = spawn_esc_listener(cancel, paused, ack);
    // Should stop without hanging
    guard.stop().await;
}

#[tokio::test]
async fn esc_listener_stops_when_cancelled() {
    let cancel = Arc::new(AtomicBool::new(false));
    let paused = Arc::new(AtomicBool::new(false));
    let ack = Arc::new(AtomicBool::new(false));
    let guard = spawn_esc_listener(Arc::clone(&cancel), paused, ack);
    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    guard.stop().await;
}

#[tokio::test]
async fn esc_listener_pauses_and_resumes() {
    use std::sync::atomic::Ordering;
    let cancel = Arc::new(AtomicBool::new(false));
    let paused = Arc::new(AtomicBool::new(false));
    let ack = Arc::new(AtomicBool::new(false));
    let guard = spawn_esc_listener(Arc::clone(&cancel), Arc::clone(&paused), Arc::clone(&ack));

    // Pause — the listener should yield raw mode
    paused.store(true, Ordering::Release);
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    assert!(ack.load(Ordering::Acquire));

    // Unpause — the listener should re-enter raw mode
    paused.store(false, Ordering::Release);
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    assert!(!ack.load(Ordering::Acquire));

    // Clean stop
    guard.stop().await;
}

#[tokio::test]
async fn esc_listener_stops_while_paused() {
    use std::sync::atomic::Ordering;
    let cancel = Arc::new(AtomicBool::new(false));
    let paused = Arc::new(AtomicBool::new(false));
    let ack = Arc::new(AtomicBool::new(false));
    let guard = spawn_esc_listener(Arc::clone(&cancel), Arc::clone(&paused), Arc::clone(&ack));

    // Pause then immediately stop — must not hang
    paused.store(true, Ordering::Release);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(ack.load(Ordering::Acquire));
    guard.stop().await;
}

#[tokio::test]
async fn esc_listener_cancel_while_paused() {
    use std::sync::atomic::Ordering;
    let cancel = Arc::new(AtomicBool::new(false));
    let paused = Arc::new(AtomicBool::new(false));
    let ack = Arc::new(AtomicBool::new(false));
    let guard = spawn_esc_listener(Arc::clone(&cancel), Arc::clone(&paused), Arc::clone(&ack));

    paused.store(true, Ordering::Release);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(ack.load(Ordering::Acquire));
    cancel.store(true, Ordering::Relaxed);
    guard.stop().await;
}

// ── /clear per-task reset ──

/// The exact sequence the two /clear handlers run (retain by role, then
/// `reset_session_for_clear`), extracted so the reset semantics are testable
/// without driving the real REPL loop.
fn run_clear_reset(agent: &mut Agent) {
    agent.messages.retain(|m| m.role == "system");
    agent.reset_session_for_clear();
}

#[tokio::test]
async fn clear_resets_per_task_state_keeping_system_prompt() {
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_response("ok")
        .build()
        .await;
    let config = crate::config::Config {
        endpoint: format!("{}/v1", server.url()),
        model: "mock-model".to_string(),
        context_length: crate::config::default_context_length(),
        agent: crate::config::AgentConfig {
            max_iterations: 4,
            step_timeout_secs: 5,
            stream_stall_timeout_secs: None,
            streaming: false,
            native_function_calling: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut agent = Agent::new(config).await.expect("agent::new");

    // Simulate a session where run_task stamped the TASK FOCUS overlay onto
    // the base system prompt (current marking scheme) and left per-task state
    // behind — the exact leak /clear must clean up.
    let base_prompt = agent.messages[0].content.text().to_string();
    let focus = "\n\n## TASK FOCUS (READ THIS FIRST)\nfix the parser now";
    agent.messages[0] = crate::api::types::Message::system(format!(
        "<selfware_focus_overlay>{focus}</selfware_focus_overlay>{}",
        base_prompt
    ));
    agent.current_task_context = "fix the parser now".to_string();
    agent.last_assistant_response = "final answer for the old task".to_string();
    agent
        .messages
        .push(crate::api::types::Message::user("old turn"));
    agent
        .messages
        .push(crate::api::types::Message::assistant("old reply"));
    agent
        .messages
        .push(crate::api::types::Message::system("extra system guidance"));
    // A failure-mode counter from the old task must also be reset.
    agent.mutating_tool_call_count = 7;

    run_clear_reset(&mut agent);

    // Base system prompt preserved — overlay stripped, back to the clean base.
    assert_eq!(
        agent.messages[0].content.text(),
        base_prompt,
        "messages[0] must return to the clean base system prompt"
    );
    assert!(
        !agent.messages[0].content.text().contains("TASK FOCUS"),
        "no stale task focus overlay may survive /clear"
    );
    // Session/system context /clear documents keeping is preserved.
    assert!(
        agent
            .messages
            .iter()
            .any(|m| m.role == "system" && m.content.text().contains("extra system guidance")),
        "other system messages must survive /clear"
    );
    assert!(
        agent.messages.iter().all(|m| m.role == "system"),
        "all non-system turns must be dropped"
    );
    // Per-task state is gone.
    assert!(
        agent.current_task_context.is_empty(),
        "current_task_context must be cleared"
    );
    assert!(
        agent.last_assistant_response.is_empty(),
        "last_assistant_response must be cleared"
    );
    assert_eq!(
        agent.mutating_tool_call_count, 0,
        "failure-mode counters must be reset for the next task"
    );

    server.stop().await;
}

#[tokio::test]
async fn clear_reset_strips_legacy_unmarked_overlay() {
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_response("ok")
        .build()
        .await;
    let config = crate::config::Config {
        endpoint: format!("{}/v1", server.url()),
        model: "mock-model".to_string(),
        context_length: crate::config::default_context_length(),
        agent: crate::config::AgentConfig {
            ..Default::default()
        },
        ..Default::default()
    };
    let mut agent = Agent::new(config).await.expect("agent::new");
    let base_prompt = agent.messages[0].content.text().to_string();
    // Old-scheme stamps have no marker; /clear must not corrupt messages[0].
    let focus = "\n\n## TASK FOCUS (READ THIS FIRST)\nold-scheme overlay";
    agent.messages[0] = crate::api::types::Message::system(format!("{}{}", focus, base_prompt));
    agent.last_assistant_response = "stale".to_string();

    run_clear_reset(&mut agent);

    // No marker → strip is a no-op for content, but per-task state resets.
    assert!(agent.messages[0].content.text().contains("TASK FOCUS"));
    assert!(agent.last_assistant_response.is_empty());
    server.stop().await;
}

// ── /resume targeted state restoration ──

/// The interactive `/resume <prefix>` handler used to swap the whole agent
/// struct (`*self = resumed`), silently dropping the live session's handles:
/// the edit-history timeline, the TUI event/stream wiring, the session-log
/// file handle, and the Ctrl+C/ESC tokens this loop captured at startup.
/// `restore_resumed_state` must commit the resumed checkpoint state INTO the
/// live session while keeping those handles.
#[tokio::test]
async fn resume_restore_replaces_task_state_but_keeps_live_handles() {
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_response("ok")
        .build()
        .await;
    let config = crate::config::Config {
        endpoint: format!("{}/v1", server.url()),
        model: "mock-model".to_string(),
        context_length: crate::config::default_context_length(),
        agent: crate::config::AgentConfig {
            max_iterations: 4,
            step_timeout_secs: 5,
            stream_stall_timeout_secs: None,
            streaming: false,
            native_function_calling: false,
            ..Default::default()
        },
        ..Default::default()
    };

    // ── The live session (the pre-resume `self`) ──
    let mut live = Agent::new(config.clone()).await.expect("agent::new");
    // Chat history accumulated before the /resume.
    live.messages
        .push(crate::api::types::Message::user("old chat turn"));
    // The REPL-side undo timeline built up over the session's edits.
    live.edit_history
        .create_checkpoint(crate::session::edit_history::EditAction::Manual {
            description: "pre-resume edit checkpoint".to_string(),
        });
    assert!(
        !live.edit_history.is_empty(),
        "sanity: live timeline is non-empty"
    );
    live.redo_stack
        .push(("undo entry".to_string(), std::collections::HashMap::new()));
    // Session-bound handles/identity we must not lose.
    let live_session_id = live
        .session_logger
        .as_ref()
        .expect("sanity: live session logger")
        .session_id()
        .to_string();
    let live_events = std::sync::Arc::clone(&live.events);
    let live_cancel = std::sync::Arc::clone(&live.cancelled);
    live.last_assistant_response = "pre-resume reply".to_string();
    live.force_non_streaming = true;
    // Stale task-budget/counters from earlier chat runs — must be replaced.
    live.prior_elapsed_secs = 7;
    live.cumulative_token_usage.total = 111;
    live.mutation_sequence = 3;

    // ── The resumed agent (what `Agent::resume` returns) ──
    let mut resumed = Agent::new(config).await.expect("agent::new");
    let expected_messages = vec![
        crate::api::types::Message::system("checkpoint system".to_string()),
        crate::api::types::Message::user("continue the checkpointed task".to_string()),
    ];
    resumed.messages = expected_messages.clone();
    resumed.current_checkpoint = Some(crate::checkpoint::TaskCheckpoint::new(
        "resume-test-task".to_string(),
        "checkpointed task".to_string(),
    ));
    // Budgets / counters `Agent::resume` restores from the checkpoint.
    resumed.prior_elapsed_secs = 42;
    resumed.cumulative_token_usage.total = 999;
    resumed.mutation_sequence = 7;
    let resumed_session_id = resumed
        .session_logger
        .as_ref()
        .expect("sanity: resumed session logger")
        .session_id()
        .to_string();
    assert_ne!(
        live_session_id, resumed_session_id,
        "sanity: the fresh agent must have its own session id"
    );

    // ── The fix under test ──
    live.restore_resumed_state(resumed);

    // (a) Message history is replaced with the checkpoint's.
    assert_eq!(
        live.messages.len(),
        expected_messages.len(),
        "message history must be replaced by the checkpoint's"
    );
    for (got, want) in live.messages.iter().zip(&expected_messages) {
        assert_eq!(got.role, want.role);
        assert_eq!(got.content.text(), want.content.text());
    }
    // (c) Task timing/budget fields are re-based from the checkpoint — the
    //     stale pre-resume chat values must not leak into the task run.
    assert_eq!(
        live.prior_elapsed_secs, 42,
        "wall-clock baseline must be the checkpoint's"
    );
    assert_eq!(
        live.cumulative_token_usage.total, 999,
        "cumulative token total must be the checkpoint's"
    );
    assert_eq!(
        live.mutation_sequence, 7,
        "guard counters must be the checkpoint's"
    );
    // The task checkpoint itself is armed on the live struct.
    assert!(live.current_checkpoint.is_some());
    assert_eq!(
        live.current_checkpoint.as_ref().unwrap().task_id,
        "resume-test-task"
    );

    // (b) REPL-side handles survive (the old `*self = resumed` swap reset each
    //     of these to a fresh agent's empty/new value).
    assert!(
        !live.edit_history.is_empty(),
        "edit-history timeline must survive the restore"
    );
    assert_eq!(
        live.redo_stack.len(),
        1,
        "redo stack must survive the restore"
    );
    assert!(live.session_logger.is_some());
    assert_eq!(
        live.session_logger.as_ref().unwrap().session_id(),
        live_session_id,
        "session-log handle must be the live session's, not the resumed agent's"
    );
    assert!(
        std::sync::Arc::ptr_eq(&live.events, &live_events),
        "TUI event-stream Arc must be preserved"
    );
    assert!(
        std::sync::Arc::ptr_eq(&live.cancelled, &live_cancel),
        "Ctrl+C token Arc must be preserved"
    );
    assert_eq!(
        live.last_assistant_response, "pre-resume reply",
        "session-owned /copy payload must survive"
    );
    assert!(
        live.force_non_streaming,
        "latched streaming decision must survive"
    );

    server.stop().await;
}
