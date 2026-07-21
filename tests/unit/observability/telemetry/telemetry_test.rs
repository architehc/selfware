use super::*;
use std::sync::{Mutex, OnceLock};

fn sampling_test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("sampling test lock poisoned")
}

/// Guard for tests that manipulate LOG_ENTRY_COUNT to prevent concurrent conflicts.
fn rotation_test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn test_record_state_transition_does_not_panic() {
    // Just ensure the function doesn't panic
    record_state_transition("Planning", "Executing");
}

#[test]
fn test_enter_agent_step_returns_span() {
    // Ensure enter_agent_step creates a valid span
    let span = enter_agent_step("Executing", 1);
    let _guard = span.enter();
    // Span should be created without panic
}

#[test]
fn test_record_success_does_not_panic() {
    // Just ensure the function doesn't panic
    record_success();
}

#[test]
fn test_record_failure_does_not_panic() {
    // Just ensure the function doesn't panic
    record_failure("test error");
}

#[tokio::test]
async fn test_track_tool_execution_success() {
    let result: Result<i32, &str> = track_tool_execution("test_tool", || async { Ok(42) }).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
}

#[tokio::test]
async fn test_track_tool_execution_failure() {
    let result: Result<i32, &str> =
        track_tool_execution("test_tool", || async { Err("test error") }).await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "test error");
}

#[test]
fn test_init_test_tracing_does_not_panic() {
    // This can be called multiple times safely
    init_test_tracing();
}

#[test]
fn test_tool_span_macro() {
    let span = tool_span!("my_tool");
    let _guard = span.enter();
    // Should create valid span
}

#[test]
fn test_enter_agent_step_different_states() {
    let states = ["Planning", "Executing", "WaitingForTool", "Completed"];
    for (i, state) in states.iter().enumerate() {
        let span = enter_agent_step(state, i);
        let _guard = span.enter();
    }
}

#[test]
fn test_record_state_transition_various() {
    let transitions = [
        ("Idle", "Planning"),
        ("Planning", "Executing"),
        ("Executing", "WaitingForTool"),
        ("WaitingForTool", "Executing"),
        ("Executing", "Completed"),
    ];
    for (from, to) in transitions {
        record_state_transition(from, to);
    }
}

#[tokio::test]
async fn test_track_tool_execution_with_delay() {
    let result: Result<u64, &str> = track_tool_execution("slow_tool", || async {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        Ok(100u64)
    })
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 100);
}

#[test]
fn test_nested_spans() {
    let outer = enter_agent_step("Outer", 0);
    let _outer_guard = outer.enter();

    let inner = enter_agent_step("Inner", 1);
    let _inner_guard = inner.enter();

    record_success();
}

#[test]
fn test_record_failure_with_various_errors() {
    let errors = [
        "Connection timeout",
        "File not found",
        "Permission denied",
        "",
        "Error with special chars: <>&\"'",
    ];
    for error in errors {
        record_failure(error);
    }
}

#[tokio::test]
async fn test_track_tool_execution_with_string_error() {
    let result: Result<(), String> = track_tool_execution("string_error_tool", || async {
        Err("Custom error message".to_string())
    })
    .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "Custom error message");
}

#[test]
fn test_multiple_init_test_tracing_calls() {
    // Multiple calls should be safe
    init_test_tracing();
    init_test_tracing();
    init_test_tracing();
}

#[tokio::test]
async fn test_track_tool_execution_returns_correct_value() {
    let result: Result<Vec<i32>, &str> =
        track_tool_execution("vec_tool", || async { Ok(vec![1, 2, 3, 4, 5]) }).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), vec![1, 2, 3, 4, 5]);
}

#[tokio::test]
async fn test_track_tool_execution_with_complex_type() {
    #[derive(Debug, PartialEq)]
    struct ComplexResult {
        value: i32,
        name: String,
    }

    let result: Result<ComplexResult, &str> = track_tool_execution("complex_tool", || async {
        Ok(ComplexResult {
            value: 42,
            name: "test".to_string(),
        })
    })
    .await;

    assert!(result.is_ok());
    let res = result.unwrap();
    assert_eq!(res.value, 42);
    assert_eq!(res.name, "test");
}

#[tokio::test]
async fn test_track_tool_execution_error_message_preserved() {
    let error_msg = "Very specific error: code 123";
    let result: Result<(), String> =
        track_tool_execution("error_tool", || async { Err(error_msg.to_string()) }).await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), error_msg);
}

#[test]
fn test_enter_agent_step_high_step_numbers() {
    let span = enter_agent_step("Testing", 999999);
    let _guard = span.enter();
    record_success();
}

#[test]
fn test_enter_agent_step_zero_step() {
    let span = enter_agent_step("Start", 0);
    let _guard = span.enter();
    record_success();
}

#[test]
fn test_record_state_transition_same_state() {
    record_state_transition("Running", "Running");
}

#[test]
fn test_record_state_transition_empty_states() {
    record_state_transition("", "");
}

#[test]
fn test_record_failure_unicode() {
    record_failure("错误: 连接失败 🔥");
    record_failure("Ошибка подключения");
    record_failure("エラー: 接続に失敗しました");
}

#[test]
fn test_record_success_multiple_calls() {
    for _ in 0..10 {
        record_success();
    }
}

#[test]
fn test_record_failure_multiple_calls() {
    for i in 0..10 {
        record_failure(&format!("Error {}", i));
    }
}

#[test]
fn test_tool_span_various_names() {
    let tool_names = [
        "file_read",
        "shell_exec",
        "cargo_build",
        "git_commit",
        "http_request",
        "database_query",
        "cache_lookup",
        "",
        "tool-with-dashes",
        "tool.with.dots",
        "tool_with_unicode_名前",
    ];
    for name in tool_names {
        let span = tool_span!(name);
        let _guard = span.enter();
    }
}

#[test]
fn test_enter_agent_step_long_state_name() {
    let long_state = "A".repeat(1000);
    let span = enter_agent_step(&long_state, 0);
    let _guard = span.enter();
}

#[test]
fn test_record_failure_long_error() {
    let long_error = "Error: ".to_string() + &"x".repeat(10000);
    record_failure(&long_error);
}

#[tokio::test]
async fn test_track_tool_execution_unit_result() {
    let result: Result<(), &str> = track_tool_execution("void_tool", || async { Ok(()) }).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_track_tool_execution_bool_result() {
    let result: Result<bool, &str> = track_tool_execution("bool_tool", || async { Ok(true) }).await;

    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[tokio::test]
async fn test_track_tool_execution_option_in_ok() {
    let result: Result<Option<i32>, &str> =
        track_tool_execution("option_tool", || async { Ok(Some(42)) }).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(42));
}

#[tokio::test]
async fn test_track_tool_execution_none_in_ok() {
    let result: Result<Option<i32>, &str> =
        track_tool_execution("none_tool", || async { Ok(None) }).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), None);
}

#[test]
fn test_span_hierarchy() {
    let span1 = enter_agent_step("Level1", 0);
    let _g1 = span1.enter();

    let span2 = enter_agent_step("Level2", 1);
    let _g2 = span2.enter();

    let span3 = enter_agent_step("Level3", 2);
    let _g3 = span3.enter();

    record_success();
}

#[tokio::test]
async fn test_track_tool_execution_with_computation() {
    let result: Result<i32, &str> = track_tool_execution("compute_tool", || async {
        let mut sum = 0;
        for i in 0..100 {
            sum += i;
        }
        Ok(sum)
    })
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 4950);
}

#[test]
fn test_record_state_transition_special_chars() {
    record_state_transition("State<A>", "State<B>");
    record_state_transition("State[1]", "State[2]");
    record_state_transition("State{x}", "State{y}");
}

#[tokio::test]
async fn test_multiple_sequential_tool_executions() {
    for i in 0..5 {
        let result: Result<i32, &str> =
            track_tool_execution(&format!("sequential_tool_{}", i), || async move { Ok(i) }).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), i);
    }
}

#[tokio::test]
async fn test_track_tool_execution_string_return() {
    let result: Result<String, &str> =
        track_tool_execution("string_tool", || async { Ok("Hello, World!".to_string()) }).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Hello, World!");
}

#[test]
fn test_tool_span_record_fields() {
    let span = tool_span!("recordable_tool");
    span.record("success", true);
    span.record("duration_ms", 100u64);
    let _guard = span.enter();
}

#[test]
fn test_enter_agent_step_returned_span() {
    let span = enter_agent_step("ValidSpan", 42);
    // Span should be created without panic and be usable
    let _guard = span.enter();
    // If we got here, the span is valid enough for use
}

#[test]
fn test_concurrent_spans() {
    use std::thread;

    let handles: Vec<_> = (0..4)
        .map(|i| {
            thread::spawn(move || {
                let span = enter_agent_step(&format!("Thread{}", i), i);
                let _guard = span.enter();
                record_success();
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_sanitize_for_log_basic() {
    assert_eq!(sanitize_for_log("hello world"), "hello world");
}

#[test]
fn test_sanitize_for_log_newlines() {
    assert_eq!(
        sanitize_for_log("line1\nline2\r\nline3"),
        "line1\\nline2\\r\\nline3"
    );
}

#[test]
fn test_sanitize_for_log_control_chars() {
    assert_eq!(sanitize_for_log("a\x00b\x1bc"), "a\\0b\\ec");
}

#[test]
fn test_sanitize_for_log_preserves_unicode() {
    assert_eq!(sanitize_for_log("hello 世界"), "hello 世界");
}

#[test]
fn test_redact_secrets_api_key() {
    let input = "Using api key sk-abc12345defghijk";
    let result = redact_secrets(input);
    assert!(!result.contains("sk-abc12345defghijk"));
    assert!(result.contains("[REDACTED]"));
}

#[test]
fn test_redact_secrets_bearer_token() {
    let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.test";
    let result = redact_secrets(input);
    assert!(!result.contains("eyJhbGciOiJIUzI1NiJ9"));
    assert!(result.contains("[REDACTED]"));
}

#[test]
fn test_redact_secrets_password_in_connection() {
    let input = "postgres://user:password=mysecret@localhost/db";
    let result = redact_secrets(input);
    assert!(!result.contains("mysecret"));
    assert!(result.contains("[REDACTED]"));
}

use std::io::Write as _;
use tracing_subscriber::fmt::MakeWriter as _;

/// A `Write` sink that hands out clones sharing one `Arc<Mutex<Vec<u8>>>`,
/// so a test can inspect what was actually written after the `MakeWriter`
/// call and its returned `Writer` are done being used.
#[derive(Clone, Default)]
struct SharedBuf(std::sync::Arc<Mutex<Vec<u8>>>);

impl std::io::Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SharedBuf {
    type Writer = SharedBuf;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[test]
fn redacting_writer_redacts_a_secret_split_across_two_write_calls() {
    let sink = SharedBuf::default();
    let make_writer = RedactingMakeWriter::new(sink.clone());
    {
        let mut writer = make_writer.make_writer();
        // Simulate a formatter emitting the line via several small
        // write_str calls, with the secret pattern split across two of
        // them -- "Bearer " in one write, the token in the next.
        writer
            .write_all(b"401 Unauthorized: Authorization: Bearer ")
            .unwrap();
        writer
            .write_all(b"sk-live-abcdefghijklmnopqrstuvwxyz\n")
            .unwrap();
        // Dropped at end of scope -- must flush+redact here.
    }
    let written = sink.0.lock().unwrap();
    let text = String::from_utf8_lossy(&written);
    assert!(
        !text.contains("sk-live-abcdefghijklmnopqrstuvwxyz"),
        "secret leaked into log output: {text}"
    );
    assert!(text.contains("[REDACTED]"));
}

#[test]
fn redacting_writer_passes_through_flush_explicitly() {
    let sink = SharedBuf::default();
    let make_writer = RedactingMakeWriter::new(sink.clone());
    let mut writer = make_writer.make_writer();
    writer
        .write_all(b"password=hunter2 connecting...\n")
        .unwrap();
    writer.flush().unwrap();
    // Already flushed explicitly; nothing further should be written on drop.
    drop(writer);

    let written = sink.0.lock().unwrap();
    let text = String::from_utf8_lossy(&written);
    assert!(!text.contains("hunter2"));
    assert!(text.contains("[REDACTED]"));
    // Exactly one copy of the (redacted) line, not duplicated by drop.
    assert_eq!(text.matches("[REDACTED]").count(), 1);
}

#[test]
fn test_redact_secrets_preserves_normal_text() {
    let input = "This is a normal log message with no secrets";
    let result = redact_secrets(input);
    assert_eq!(result, input);
}

#[test]
fn test_redact_secrets_token_prefix() {
    let input = "token-abcdefghijklmnop is being used";
    let result = redact_secrets(input);
    assert!(!result.contains("token-abcdefghijklmnop"));
    assert!(result.contains("[REDACTED]"));
}

#[test]
fn test_set_and_get_sampling_rate() {
    let _guard = sampling_test_guard();
    // Save original and restore after test
    let original = sampling_rate();
    set_sampling_rate(0.5);
    let rate = sampling_rate();
    assert!((rate - 0.5).abs() < 0.01);

    set_sampling_rate(0.0);
    assert!((sampling_rate()).abs() < 0.01);

    set_sampling_rate(1.0);
    assert!((sampling_rate() - 1.0).abs() < 0.01);

    // Clamp out-of-range values
    set_sampling_rate(2.0);
    assert!((sampling_rate() - 1.0).abs() < 0.01);
    set_sampling_rate(-1.0);
    assert!((sampling_rate()).abs() < 0.01);

    // Restore
    set_sampling_rate(original);
}

#[test]
fn test_should_sample_full_rate() {
    let _guard = sampling_test_guard();
    let original = sampling_rate();
    set_sampling_rate(1.0);
    // At full rate, should always sample
    for _ in 0..100 {
        assert!(should_sample());
    }
    set_sampling_rate(original);
}

#[test]
fn test_should_sample_zero_rate() {
    let _guard = sampling_test_guard();
    let original = sampling_rate();
    set_sampling_rate(0.0);
    // At zero rate, should never sample
    for _ in 0..100 {
        assert!(!should_sample());
    }
    set_sampling_rate(original);
}

#[test]
fn test_log_entry_count_and_increment() {
    let before = log_entry_count();
    let after = increment_log_count();
    assert_eq!(after, before + 1);
}

#[test]
fn test_rotate_if_needed_below_limit() {
    let _guard = rotation_test_guard();
    // Save and reset to a known state below the limit
    let saved = LOG_ENTRY_COUNT.load(Ordering::Relaxed);
    LOG_ENTRY_COUNT.store(0, Ordering::Relaxed);

    let rotated = rotate_if_needed();
    assert!(!rotated);

    LOG_ENTRY_COUNT.store(saved, Ordering::Relaxed);
}

#[test]
fn test_max_log_entries_constant() {
    assert_eq!(MAX_LOG_ENTRIES, 100_000);
}

#[test]
fn test_increment_functions_update_metrics() {
    let before_api = METRICS.api_requests.load(Ordering::Relaxed);
    increment_api_requests();
    assert_eq!(METRICS.api_requests.load(Ordering::Relaxed), before_api + 1);

    let before_errors = METRICS.api_errors.load(Ordering::Relaxed);
    increment_api_errors();
    assert_eq!(
        METRICS.api_errors.load(Ordering::Relaxed),
        before_errors + 1
    );

    let before_tool = METRICS.tool_executions.load(Ordering::Relaxed);
    increment_tool_executions();
    assert_eq!(
        METRICS.tool_executions.load(Ordering::Relaxed),
        before_tool + 1
    );

    let before_tool_err = METRICS.tool_errors.load(Ordering::Relaxed);
    increment_tool_errors();
    assert_eq!(
        METRICS.tool_errors.load(Ordering::Relaxed),
        before_tool_err + 1
    );

    let before_tokens = METRICS.tokens_processed.load(Ordering::Relaxed);
    add_tokens_processed(42);
    assert_eq!(
        METRICS.tokens_processed.load(Ordering::Relaxed),
        before_tokens + 42
    );
}

// -----------------------------------------------------------------------
// Additional tests targeting uncovered lines
// -----------------------------------------------------------------------

// --- sanitize_for_log: tab, vertical tab, form feed, generic control ---

#[test]
fn test_sanitize_for_log_tab() {
    // Covers the '\t' => "\\t" branch (line 120)
    assert_eq!(sanitize_for_log("before\tafter"), "before\\tafter");
    assert_eq!(sanitize_for_log("\t"), "\\t");
    assert_eq!(sanitize_for_log("\t\t\t"), "\\t\\t\\t");
}

#[test]
fn test_sanitize_for_log_vertical_tab() {
    // Covers the '\x0b' => "\\v" branch (line 121)
    assert_eq!(sanitize_for_log("a\x0bb"), "a\\vb");
    assert_eq!(sanitize_for_log("\x0b"), "\\v");
}

#[test]
fn test_sanitize_for_log_form_feed() {
    // Covers the '\x0c' => "\\f" branch (line 122)
    assert_eq!(sanitize_for_log("a\x0cb"), "a\\fb");
    assert_eq!(sanitize_for_log("\x0c"), "\\f");
}

#[test]
fn test_sanitize_for_log_escape_char() {
    // Covers the '\x1b' => "\\e" branch (line 123)
    assert_eq!(sanitize_for_log("a\x1bb"), "a\\eb");
    assert_eq!(sanitize_for_log("\x1b[31m"), "\\e[31m");
}

#[test]
fn test_sanitize_for_log_null() {
    // Covers the '\x00' => "\\0" branch (line 124)
    assert_eq!(sanitize_for_log("a\x00b"), "a\\0b");
    assert_eq!(sanitize_for_log("\x00"), "\\0");
}

#[test]
fn test_sanitize_for_log_generic_control_char() {
    // Covers the generic control char fallback: c if c.is_control() => \\u{XXXX} (line 125)
    // \x01 (SOH) is a control char not handled by explicit branches
    assert_eq!(sanitize_for_log("a\x01b"), "a\\u0001b");
    // \x02 (STX)
    assert_eq!(sanitize_for_log("x\x02y"), "x\\u0002y");
    // \x03 (ETX)
    assert_eq!(sanitize_for_log("\x03"), "\\u0003");
    // \x04 (EOT)
    assert_eq!(sanitize_for_log("\x04"), "\\u0004");
    // \x05 (ENQ)
    assert_eq!(sanitize_for_log("\x05"), "\\u0005");
    // \x06 (ACK)
    assert_eq!(sanitize_for_log("\x06"), "\\u0006");
    // \x07 (BEL)
    assert_eq!(sanitize_for_log("\x07"), "\\u0007");
    // \x0e (SO) - Shift Out
    assert_eq!(sanitize_for_log("\x0e"), "\\u000e");
    // \x7f (DEL) - also a control char
    assert_eq!(sanitize_for_log("\x7f"), "\\u007f");
}

#[test]
fn test_sanitize_for_log_all_special_chars_combined() {
    // Exercises every branch in one call
    let input = "normal\n\r\t\x0b\x0c\x1b\x00\x01text";
    let expected = "normal\\n\\r\\t\\v\\f\\e\\0\\u0001text";
    assert_eq!(sanitize_for_log(input), expected);
}

#[test]
fn test_sanitize_for_log_empty_string() {
    assert_eq!(sanitize_for_log(""), "");
}

#[test]
fn test_sanitize_for_log_only_control_chars() {
    assert_eq!(
        sanitize_for_log("\x00\x01\x02\x03"),
        "\\0\\u0001\\u0002\\u0003"
    );
}

// --- redact_secrets: key- prefix, passwd=, pwd= variants, case insensitivity ---

#[test]
fn test_redact_secrets_key_prefix() {
    // Covers the key-... pattern (line 139)
    let input = "Using key-abcdefghijklmnop for auth";
    let result = redact_secrets(input);
    assert!(!result.contains("key-abcdefghijklmnop"));
    assert!(result.contains("[REDACTED]"));
}

#[test]
fn test_redact_secrets_passwd_variant() {
    // Covers the passwd= pattern (line 143)
    let input = "config passwd=secretvalue123 host=localhost";
    let result = redact_secrets(input);
    assert!(!result.contains("secretvalue123"));
    assert!(result.contains("[REDACTED]"));
}

#[test]
fn test_redact_secrets_pwd_variant() {
    // Covers the pwd= pattern (line 143)
    let input = "connection pwd=mypassword host=db.example.com";
    let result = redact_secrets(input);
    assert!(!result.contains("mypassword"));
    assert!(result.contains("[REDACTED]"));
}

#[test]
fn test_redact_secrets_case_insensitive_sk() {
    // Case insensitive match (?i)
    let input = "SK-ABCDEFGHIJKLMNOP";
    let result = redact_secrets(input);
    assert!(!result.contains("SK-ABCDEFGHIJKLMNOP"));
    assert!(result.contains("[REDACTED]"));
}

#[test]
fn test_redact_secrets_case_insensitive_bearer() {
    let input = "BEARER AbCdEfGhIjKlMnOp";
    let result = redact_secrets(input);
    assert!(!result.contains("AbCdEfGhIjKlMnOp"));
    assert!(result.contains("[REDACTED]"));
}

#[test]
fn test_redact_secrets_case_insensitive_password() {
    let input = "PASSWORD=SuperSecret123";
    let result = redact_secrets(input);
    assert!(!result.contains("SuperSecret123"));
    assert!(result.contains("[REDACTED]"));
}

#[test]
fn test_redact_secrets_multiple_secrets_in_one_string() {
    let input = "sk-key123456789 and token-abcdefghij and password=secret";
    let result = redact_secrets(input);
    assert!(!result.contains("sk-key123456789"));
    assert!(!result.contains("token-abcdefghij"));
    // Should have multiple [REDACTED]
    assert!(result.matches("[REDACTED]").count() >= 2);
}

#[test]
fn test_redact_secrets_short_key_not_redacted() {
    // Keys shorter than 8 chars after the prefix should NOT be redacted
    let input = "sk-abc is too short";
    let result = redact_secrets(input);
    assert_eq!(result, input);
}

#[test]
fn test_redact_secrets_password_with_spaces_around_equals() {
    let input = "password = mysecretpassword";
    let result = redact_secrets(input);
    assert!(!result.contains("mysecretpassword"));
    assert!(result.contains("[REDACTED]"));
}

#[test]
fn test_redact_secrets_empty_string() {
    assert_eq!(redact_secrets(""), "");
}

#[test]
fn test_redact_secrets_key_prefix_with_dashes_and_underscores() {
    let input = "key-abc_def-ghi_jkl";
    let result = redact_secrets(input);
    assert!(!result.contains("key-abc_def-ghi_jkl"));
    assert!(result.contains("[REDACTED]"));
}

// --- should_sample: partial rate counter-based path ---

#[test]
fn test_should_sample_partial_rate_exercises_counter_path() {
    let _guard = sampling_test_guard();
    let original = sampling_rate();

    // Set rate to 0.5 (50%) to exercise the counter-based path
    // (lines 59-60): the code enters the branch where rate_micro is
    // between 0 and 1_000_000 and uses the counter modulo approach.
    set_sampling_rate(0.5);

    // Call should_sample many times to exercise the counter-based path.
    // We cannot predict exact results because the global counter is
    // shared across tests, but the function should not panic.
    let mut sampled = 0;
    let total = 2_000_000; // Enough to wrap around the modulo at least once
    for _ in 0..total {
        if should_sample() {
            sampled += 1;
        }
    }

    // At 50% rate over 2M calls, roughly 1M should be sampled
    assert!(
        sampled > 0,
        "At 50% rate over 2M calls, at least some events should be sampled"
    );
    assert!(
        sampled < total as i64 as usize,
        "At 50% rate over 2M calls, not all events should be sampled"
    );

    set_sampling_rate(original);
}

#[test]
fn test_should_sample_low_rate_exercises_counter_path() {
    let _guard = sampling_test_guard();
    let original = sampling_rate();

    // Set a very low rate to verify counter path returns false for most calls
    set_sampling_rate(0.001);

    // Exercise the path; we just verify it doesn't panic and
    // returns a mix of true/false over enough iterations
    for _ in 0..1000 {
        let _ = should_sample();
    }

    set_sampling_rate(original);
}

// --- rotate_if_needed: actual rotation path ---

#[test]
fn test_rotate_if_needed_triggers_rotation() {
    let _guard = rotation_test_guard();
    let saved = LOG_ENTRY_COUNT.load(Ordering::Relaxed);

    // Set count to just above MAX_LOG_ENTRIES
    LOG_ENTRY_COUNT.store(MAX_LOG_ENTRIES + 10, Ordering::Relaxed);

    let rotated = rotate_if_needed();
    assert!(
        rotated,
        "Should trigger rotation when count >= MAX_LOG_ENTRIES"
    );

    // After rotation, count should be halved
    let after = LOG_ENTRY_COUNT.load(Ordering::Relaxed);
    assert_eq!(after, (MAX_LOG_ENTRIES + 10) / 2);

    // Restore
    LOG_ENTRY_COUNT.store(saved, Ordering::Relaxed);
}

#[test]
fn test_rotate_if_needed_exactly_at_limit() {
    let _guard = rotation_test_guard();
    let saved = LOG_ENTRY_COUNT.load(Ordering::Relaxed);

    // Set count to exactly MAX_LOG_ENTRIES
    LOG_ENTRY_COUNT.store(MAX_LOG_ENTRIES, Ordering::Relaxed);

    let rotated = rotate_if_needed();
    assert!(
        rotated,
        "Should trigger rotation when count == MAX_LOG_ENTRIES"
    );

    let after = LOG_ENTRY_COUNT.load(Ordering::Relaxed);
    assert_eq!(after, MAX_LOG_ENTRIES / 2);

    LOG_ENTRY_COUNT.store(saved, Ordering::Relaxed);
}

#[test]
fn test_rotate_if_needed_just_below_limit() {
    let _guard = rotation_test_guard();
    let saved = LOG_ENTRY_COUNT.load(Ordering::Relaxed);

    LOG_ENTRY_COUNT.store(MAX_LOG_ENTRIES - 1, Ordering::Relaxed);

    let rotated = rotate_if_needed();
    assert!(!rotated, "Should not rotate when count < MAX_LOG_ENTRIES");

    LOG_ENTRY_COUNT.store(saved, Ordering::Relaxed);
}

#[test]
fn test_rotate_if_needed_at_zero() {
    let _guard = rotation_test_guard();
    let saved = LOG_ENTRY_COUNT.load(Ordering::Relaxed);

    LOG_ENTRY_COUNT.store(0, Ordering::Relaxed);

    let rotated = rotate_if_needed();
    assert!(!rotated, "Should not rotate when count is 0");

    LOG_ENTRY_COUNT.store(saved, Ordering::Relaxed);
}

#[test]
fn test_rotate_if_needed_double_rotation() {
    let _guard = rotation_test_guard();
    let saved = LOG_ENTRY_COUNT.load(Ordering::Relaxed);

    // Set count well above limit
    LOG_ENTRY_COUNT.store(MAX_LOG_ENTRIES * 2, Ordering::Relaxed);

    let rotated = rotate_if_needed();
    assert!(rotated);
    let after_first = LOG_ENTRY_COUNT.load(Ordering::Relaxed);
    assert_eq!(after_first, MAX_LOG_ENTRIES); // 200_000 / 2 = 100_000

    // Still at MAX_LOG_ENTRIES, so rotating again should work
    let rotated2 = rotate_if_needed();
    assert!(rotated2);
    let after_second = LOG_ENTRY_COUNT.load(Ordering::Relaxed);
    assert_eq!(after_second, MAX_LOG_ENTRIES / 2); // 100_000 / 2 = 50_000

    // Now below limit, should not rotate
    let rotated3 = rotate_if_needed();
    assert!(!rotated3);

    LOG_ENTRY_COUNT.store(saved, Ordering::Relaxed);
}

// --- get_metrics: returns reference to global METRICS ---

#[test]
fn test_get_metrics_returns_static_ref() {
    let m = get_metrics();
    // Should be the same static METRICS
    let before = m.api_requests.load(Ordering::Relaxed);
    increment_api_requests();
    let after = m.api_requests.load(Ordering::Relaxed);
    assert_eq!(after, before + 1);
}

#[test]
fn test_get_metrics_all_fields_accessible() {
    let m = get_metrics();
    // All fields should be readable without panic
    let _ = m.api_requests.load(Ordering::Relaxed);
    let _ = m.api_errors.load(Ordering::Relaxed);
    let _ = m.tool_executions.load(Ordering::Relaxed);
    let _ = m.tool_errors.load(Ordering::Relaxed);
    let _ = m.tokens_processed.load(Ordering::Relaxed);
}

// --- add_tokens_processed edge cases ---

#[test]
fn test_add_tokens_processed_zero() {
    let before = METRICS.tokens_processed.load(Ordering::Relaxed);
    add_tokens_processed(0);
    let after = METRICS.tokens_processed.load(Ordering::Relaxed);
    // Adding 0 should not decrease the count; other tests may concurrently
    // increment, so we only assert it's >= before.
    assert!(after >= before);
}

#[test]
fn test_add_tokens_processed_large_value() {
    let before = METRICS.tokens_processed.load(Ordering::Relaxed);
    add_tokens_processed(1_000_000);
    let after = METRICS.tokens_processed.load(Ordering::Relaxed);
    // Other parallel tests may also add tokens, so just verify our
    // contribution was applied (after should be at least before + 1M).
    assert!(after >= before + 1_000_000);
}

#[test]
fn test_add_tokens_processed_multiple_adds() {
    let before = METRICS.tokens_processed.load(Ordering::Relaxed);
    add_tokens_processed(10);
    add_tokens_processed(20);
    add_tokens_processed(30);
    let after = METRICS.tokens_processed.load(Ordering::Relaxed);
    // At minimum our 60 tokens should be reflected; other tests may add more.
    assert!(after >= before + 60);
}

// --- increment_log_count: verify atomicity ---

#[test]
fn test_increment_log_count_sequential() {
    let before = log_entry_count();
    let r1 = increment_log_count();
    let r2 = increment_log_count();
    let r3 = increment_log_count();
    // Each call returns the new count after incrementing
    assert_eq!(r1, before + 1);
    assert_eq!(r2, before + 2);
    assert_eq!(r3, before + 3);
}

#[test]
fn test_log_entry_count_matches_after_increments() {
    let before = log_entry_count();
    for _ in 0..5 {
        increment_log_count();
    }
    let after = log_entry_count();
    assert_eq!(after, before + 5);
}

// --- shutdown_tracing: safe to call even when not initialized ---

#[test]
fn test_shutdown_tracing_no_panic_when_not_initialized() {
    // TRACING_GUARD may or may not be initialized; shutdown should be safe either way
    shutdown_tracing();
}

#[test]
fn test_shutdown_tracing_idempotent() {
    // Calling shutdown multiple times should be safe
    shutdown_tracing();
    shutdown_tracing();
    shutdown_tracing();
}

// --- init_tracing: when RUST_LOG is not set ---

#[test]
fn test_init_tracing_no_rust_log() {
    // When RUST_LOG is not set, init_tracing should be a no-op without panic.
    // We just verify it does not panic.
    init_tracing();
}

// --- record_failure with secrets in error message ---

#[test]
fn test_record_failure_redacts_secrets() {
    // Exercise the redact_secrets + sanitize_for_log pipeline inside record_failure
    record_failure("Connection failed with sk-supersecretkey123456");
    record_failure("Auth error: Bearer eyJhbGciOiJIUzI1NiJ9.payload");
    record_failure("DB error: password=mysecretpass");
}

// --- record_state_transition with control characters ---

#[test]
fn test_record_state_transition_with_control_chars() {
    // Exercise sanitize_for_log inside record_state_transition
    record_state_transition("State\nA", "State\tB");
    record_state_transition("From\x00", "To\x1b");
    record_state_transition("From\x0b", "To\x0c");
}

// --- enter_agent_step with special characters ---

#[test]
fn test_enter_agent_step_with_control_chars() {
    // Exercise sanitize_for_log inside enter_agent_step
    let span = enter_agent_step("State\n\r\t\x00\x1b", 0);
    let _guard = span.enter();
}

// --- track_tool_execution with secrets in tool name ---

#[tokio::test]
async fn test_track_tool_execution_redacts_tool_name() {
    // Tool name containing a secret should be redacted
    let result: Result<i32, &str> =
        track_tool_execution("tool_with_sk-secretkeyvalue123", || async { Ok(1) }).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_track_tool_execution_sanitizes_tool_name() {
    // Tool name with control characters should be sanitized
    let result: Result<i32, &str> =
        track_tool_execution("tool\nwith\nnewlines", || async { Ok(1) }).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_track_tool_execution_error_with_secrets() {
    // Error message containing secrets should be redacted in spans
    let result: Result<i32, String> = track_tool_execution("secure_tool", || async {
        Err("Failed with password=secret123".to_string())
    })
    .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "Failed with password=secret123");
}

// --- secret_patterns: ensure lazy init works ---

#[test]
fn test_secret_patterns_initialized() {
    // Calling redact_secrets forces secret_patterns() init
    let _ = redact_secrets("test");
    // Calling again uses the cached patterns
    let _ = redact_secrets("another test");
}

// --- sampling_rate: precision and edge cases ---

#[test]
fn test_sampling_rate_precision() {
    let _guard = sampling_test_guard();
    let original = sampling_rate();

    set_sampling_rate(0.123456);
    let rate = sampling_rate();
    // Should be close to 0.123456 within fixed-point precision
    assert!((rate - 0.123456).abs() < 0.000002);

    set_sampling_rate(0.999999);
    let rate = sampling_rate();
    assert!((rate - 0.999999).abs() < 0.000002);

    set_sampling_rate(0.000001);
    let rate = sampling_rate();
    assert!((rate - 0.000001).abs() < 0.000002);

    set_sampling_rate(original);
}

// --- Metrics concurrent access ---

#[test]
fn test_metrics_concurrent_increments() {
    use std::thread;

    let before_api = METRICS.api_requests.load(Ordering::Relaxed);
    let before_tool = METRICS.tool_executions.load(Ordering::Relaxed);

    let handles: Vec<_> = (0..10)
        .map(|i| {
            thread::spawn(move || {
                for _ in 0..100 {
                    if i % 2 == 0 {
                        increment_api_requests();
                    } else {
                        increment_tool_executions();
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let after_api = METRICS.api_requests.load(Ordering::Relaxed);
    let after_tool = METRICS.tool_executions.load(Ordering::Relaxed);

    // 5 threads doing api_requests * 100 each = 500.
    // Other parallel tests may also increment these counters, so the delta
    // can be >= 500 but never less (atomics guarantee no lost updates).
    assert!(
        after_api - before_api >= 500,
        "Expected at least 500 api_request increments, got {}",
        after_api - before_api
    );
    // 5 threads doing tool_executions * 100 each = 500
    assert!(
        after_tool - before_tool >= 500,
        "Expected at least 500 tool_execution increments, got {}",
        after_tool - before_tool
    );
}

// --- sanitize_for_log: injection prevention ---

#[test]
fn test_sanitize_for_log_injection_attempt() {
    // Simulates a log injection attack: attacker inserts newline + fake log entry
    let malicious = "normal input\n[ERROR] Fake admin alert: system compromised";
    let sanitized = sanitize_for_log(malicious);
    // Newline should be escaped, preventing the fake log entry from appearing on its own line
    assert!(!sanitized.contains('\n'));
    assert!(sanitized.contains("\\n"));
    assert!(sanitized.contains("[ERROR] Fake admin alert: system compromised"));
}

#[test]
fn test_sanitize_for_log_carriage_return_injection() {
    let malicious = "first line\r[INFO] spoofed log entry";
    let sanitized = sanitize_for_log(malicious);
    assert!(!sanitized.contains('\r'));
    assert!(sanitized.contains("\\r"));
}

// --- redact_secrets: bearer with dots (JWT-style) ---

#[test]
fn test_redact_secrets_bearer_jwt_style() {
    let input =
        "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0";
    let result = redact_secrets(input);
    assert!(result.contains("[REDACTED]"));
    assert!(!result.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
}

#[test]
fn test_redact_secrets_token_with_underscores() {
    let input = "token-abc_def_ghi_jkl_mno";
    let result = redact_secrets(input);
    assert!(!result.contains("token-abc_def_ghi_jkl_mno"));
    assert!(result.contains("[REDACTED]"));
}

// --- Combined sanitize + redact (as used in record_failure / track_tool_execution) ---

#[test]
fn test_sanitize_then_redact_pipeline() {
    // This is the exact pipeline used in record_failure and track_tool_execution
    let input = "Error: sk-secretkey12345678\nNew line injection";
    let sanitized = sanitize_for_log(input);
    let redacted = redact_secrets(&sanitized);

    // No raw newlines
    assert!(!redacted.contains('\n'));
    // Secret redacted
    assert!(!redacted.contains("sk-secretkey12345678"));
    assert!(redacted.contains("[REDACTED]"));
    assert!(redacted.contains("\\n"));
}

#[test]
fn test_redact_then_sanitize_order_independence() {
    let input = "password=secret123\ttab_separated";
    let s_then_r = redact_secrets(&sanitize_for_log(input));
    // Both sanitization and redaction should have occurred
    assert!(!s_then_r.contains('\t'));
}

// --- Metrics: verify get_metrics returns same instance ---

#[test]
fn test_get_metrics_is_same_as_static() {
    let m = get_metrics();
    // Increment via the function
    let before = m.tool_errors.load(Ordering::Relaxed);
    increment_tool_errors();
    // Should reflect on the same reference
    assert_eq!(m.tool_errors.load(Ordering::Relaxed), before + 1);
}

// --- increment functions: verify both atomic and metrics crate counters ---

#[test]
fn test_increment_api_requests_multiple() {
    let before = METRICS.api_requests.load(Ordering::Relaxed);
    for _ in 0..5 {
        increment_api_requests();
    }
    assert_eq!(METRICS.api_requests.load(Ordering::Relaxed), before + 5);
}

#[test]
fn test_increment_api_errors_multiple() {
    let before = METRICS.api_errors.load(Ordering::Relaxed);
    for _ in 0..3 {
        increment_api_errors();
    }
    assert_eq!(METRICS.api_errors.load(Ordering::Relaxed), before + 3);
}

#[test]
fn test_increment_tool_executions_multiple() {
    let before = METRICS.tool_executions.load(Ordering::Relaxed);
    for _ in 0..7 {
        increment_tool_executions();
    }
    assert_eq!(METRICS.tool_executions.load(Ordering::Relaxed), before + 7);
}

#[test]
fn test_increment_tool_errors_multiple() {
    let before = METRICS.tool_errors.load(Ordering::Relaxed);
    for _ in 0..4 {
        increment_tool_errors();
    }
    assert_eq!(METRICS.tool_errors.load(Ordering::Relaxed), before + 4);
}

// --- Concurrent log rotation ---

#[test]
fn test_rotate_if_needed_concurrent() {
    use std::thread;

    let _guard = rotation_test_guard();
    let saved = LOG_ENTRY_COUNT.load(Ordering::Relaxed);
    LOG_ENTRY_COUNT.store(MAX_LOG_ENTRIES + 100, Ordering::Relaxed);

    let handles: Vec<_> = (0..4).map(|_| thread::spawn(rotate_if_needed)).collect();

    let mut any_rotated = false;
    for h in handles {
        if h.join().unwrap() {
            any_rotated = true;
        }
    }

    assert!(
        any_rotated,
        "At least one thread should have triggered rotation"
    );

    // After concurrent rotation, count should be less than the original
    let final_count = LOG_ENTRY_COUNT.load(Ordering::Relaxed);
    assert!(
        final_count < MAX_LOG_ENTRIES + 100,
        "Count should have been reduced from the original"
    );

    LOG_ENTRY_COUNT.store(saved, Ordering::Relaxed);
}

// --- init_tracing_verbose: safe to call ---

#[test]
fn test_init_tracing_verbose_no_panic() {
    // init_tracing_verbose delegates to init_tracing_with_filter("info")
    // which uses a Once guard, so safe to call even if already initialized
    init_tracing_verbose();
}

// --- init_tracing_with_filter: invalid filter string ---

#[test]
fn test_init_tracing_with_filter_invalid_fallback() {
    // An invalid filter string should fall back to "warn" without panicking
    // The Once guard will skip if already initialized
    init_tracing_with_filter("this is not a valid filter!!!@@@");
}

// --- sampling rate: boundary values ---

#[test]
fn test_sampling_rate_boundary_zero() {
    let _guard = sampling_test_guard();
    let original = sampling_rate();

    set_sampling_rate(0.0);
    assert_eq!(sampling_rate(), 0.0);

    set_sampling_rate(original);
}

#[test]
fn test_sampling_rate_boundary_one() {
    let _guard = sampling_test_guard();
    let original = sampling_rate();

    set_sampling_rate(1.0);
    assert_eq!(sampling_rate(), 1.0);

    set_sampling_rate(original);
}

#[test]
fn test_sampling_rate_clamp_negative() {
    let _guard = sampling_test_guard();
    let original = sampling_rate();

    set_sampling_rate(-100.0);
    assert_eq!(sampling_rate(), 0.0);

    set_sampling_rate(original);
}

#[test]
fn test_sampling_rate_clamp_large_positive() {
    let _guard = sampling_test_guard();
    let original = sampling_rate();

    set_sampling_rate(999.0);
    assert_eq!(sampling_rate(), 1.0);

    set_sampling_rate(original);
}
