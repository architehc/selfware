    use super::*;

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
        for input in &["exit", "quit", "/exit", "/quit", "q", "/q"] {
            assert!(is_exit_command(input), "'{}' should trigger exit", input);
        }

        for input in &[
            "exiting",
            "quitting",
            "EXIT",
            "exit now",
            "query",
            "/question",
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
}
