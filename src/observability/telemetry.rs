//! Telemetry & Observability
//!
//! Provides structured logging and tracing for agent operations.
//! Features:
//! - Tool execution spans with timing
//! - Agent state transition logging
//! - Success/failure recording
//! - Configurable log levels via RUST_LOG
//! - Configurable sampling rate for non-error events
//! - Log rotation with configurable entry limits

use regex::Regex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::Instant;
use tracing::{error, info, info_span, Span};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Maximum number of in-memory log entries before rotation.
/// When this limit is reached, `rotate_if_needed()` will discard the oldest half.
pub const MAX_LOG_ENTRIES: usize = 100_000;

/// Global telemetry sampling rate stored as fixed-point (rate * 1_000_000).
/// Defaults to 1_000_000 (= 1.0 = 100%). When set below 1.0, only a fraction
/// of non-error events are logged.
static SAMPLING_RATE_MICRO: AtomicU64 = AtomicU64::new(1_000_000);

/// Simple counter for deterministic sampling when rand is not desired.
static SAMPLE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Set the telemetry sampling rate. `rate` must be in `0.0..=1.0`.
/// A rate of 1.0 means all events are logged; 0.5 means ~50% of non-error
/// events are logged.
pub fn set_sampling_rate(rate: f64) {
    let clamped = rate.clamp(0.0, 1.0);
    SAMPLING_RATE_MICRO.store((clamped * 1_000_000.0) as u64, Ordering::Relaxed);
}

/// Get the current telemetry sampling rate as a float in `0.0..=1.0`.
pub fn sampling_rate() -> f64 {
    SAMPLING_RATE_MICRO.load(Ordering::Relaxed) as f64 / 1_000_000.0
}

/// Returns `true` if the current non-error event should be sampled (logged).
/// Always returns `true` when the rate is 1.0. Uses a simple counter-based
/// approach that is deterministic and does not require the `rand` crate at
/// this call site.
pub fn should_sample() -> bool {
    let rate_micro = SAMPLING_RATE_MICRO.load(Ordering::Relaxed);
    if rate_micro >= 1_000_000 {
        return true;
    }
    if rate_micro == 0 {
        return false;
    }
    // Counter-based: sample if (counter % 1_000_000) < rate_micro
    let count = SAMPLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    (count % 1_000_000) < rate_micro
}

/// In-memory log entry buffer for rotation tracking.
static LOG_ENTRY_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Increment the in-memory log entry counter and return the new count.
pub fn increment_log_count() -> usize {
    LOG_ENTRY_COUNT.fetch_add(1, Ordering::Relaxed) + 1
}

/// Get current log entry count.
pub fn log_entry_count() -> usize {
    LOG_ENTRY_COUNT.load(Ordering::Relaxed)
}

/// Check if log rotation is needed and perform it.
/// Returns `true` if rotation was triggered (i.e., entries exceeded `MAX_LOG_ENTRIES`).
/// In the in-memory case this resets the counter to simulate discarding old entries.
/// Callers that maintain their own log buffers should drain old entries when this
/// returns `true`.
pub fn rotate_if_needed() -> bool {
    let mut count = LOG_ENTRY_COUNT.load(Ordering::Relaxed);
    loop {
        if count >= MAX_LOG_ENTRIES {
            match LOG_ENTRY_COUNT.compare_exchange_weak(
                count,
                count / 2,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    info!(
                        "Telemetry log rotation triggered: {} entries exceeded limit, reset to {}",
                        count,
                        count / 2
                    );
                    return true;
                }
                Err(actual) => count = actual,
            }
        } else {
            return false;
        }
    }
}

/// Sanitize a string for safe log output by escaping control characters.
/// Prevents log injection where attackers embed newlines to forge log entries.
pub fn sanitize_for_log(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x0b' => out.push_str("\\v"),
            '\x0c' => out.push_str("\\f"),
            '\x1b' => out.push_str("\\e"),
            '\x00' => out.push_str("\\0"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            _ => out.push(c),
        }
    }
    out
}

/// Compiled regex patterns for secret redaction.
static SECRET_PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();

fn secret_patterns() -> &'static Vec<Regex> {
    SECRET_PATTERNS.get_or_init(|| {
        vec![
            // API keys: sk-..., key-..., token-... followed by alphanumeric chars
            Regex::new(r"(?i)(sk-|key-|token-)[A-Za-z0-9_\-]{8,}").expect("invalid secret regex"),
            // Bearer tokens in Authorization headers
            Regex::new(r"(?i)Bearer\s+[A-Za-z0-9_\-\.]{8,}").expect("invalid bearer regex"),
            // Passwords in connection strings: password=..., passwd=..., pwd=...
            Regex::new(r"(?i)(password|passwd|pwd)\s*=\s*\S+").expect("invalid password regex"),
        ]
    })
}

/// Redact sensitive data patterns from a string before logging.
///
/// Matches API keys (`sk-`, `key-`, `token-` prefixed), Bearer tokens,
/// and passwords in connection strings, replacing them with `[REDACTED]`.
pub fn redact_secrets(input: &str) -> String {
    let mut result = input.to_string();
    for pattern in secret_patterns() {
        result = pattern.replace_all(&result, "[REDACTED]").to_string();
    }
    result
}

/// Initialize global tracing subscriber with configurable output
/// By default, only enables tracing if RUST_LOG is explicitly set
pub fn init_tracing() {
    // Only initialize verbose tracing if RUST_LOG is set
    // Otherwise use a quiet "error-only" mode to avoid polluting CLI output
    if std::env::var("RUST_LOG").is_ok() {
        init_tracing_with_filter(&std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()));
    }
    // If RUST_LOG not set, don't initialize tracing at all - keeps CLI output clean
}

/// Initialize tracing only for debug/verbose mode
pub fn init_tracing_verbose() {
    init_tracing_with_filter("info")
}

/// Initialize with custom filter string
pub fn init_tracing_with_filter(filter: &str) {
    // Skip if already initialized
    use std::sync::Once;
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_target(false)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_file(false)
            .with_line_number(false)
            .with_level(true)
            .compact()
            .with_writer(std::io::stderr); // Write to stderr, not stdout

        let filter_layer = EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("warn"));

        let _ = tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .try_init();
    });
}

/// Create a span for tracking tool execution with automatic duration and outcome logging
#[macro_export]
macro_rules! tool_span {
    ($tool_name:expr) => {
        tracing::info_span!(
            "tool_execution",
            tool_name = $tool_name,
            duration_ms = tracing::field::Empty,
            success = tracing::field::Empty,
            error = tracing::field::Empty,
        )
    };
}

/// Middleware for tracking tool execution with full observability
pub async fn track_tool_execution<F, Fut, T, E>(tool_name: &str, f: F) -> Result<T, E>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let start = Instant::now();
    let safe_name = redact_secrets(&sanitize_for_log(tool_name));
    let span = info_span!(
        "tool.execute",
        tool_name = safe_name.as_str(),
        duration_ms = tracing::field::Empty,
        success = tracing::field::Empty,
        error = tracing::field::Empty,
    );

    let _enter = span.enter();
    info!("Starting tool execution");

    match f().await {
        Ok(result) => {
            let duration = start.elapsed().as_millis() as u64;
            span.record("duration_ms", duration);
            span.record("success", true);
            info!(
                duration_ms = duration,
                "Tool execution completed successfully"
            );
            Ok(result)
        }
        Err(e) => {
            let duration = start.elapsed().as_millis() as u64;
            let safe_err = redact_secrets(&sanitize_for_log(&e.to_string()));
            span.record("duration_ms", duration);
            span.record("success", false);
            span.record("error", safe_err.as_str());
            error!(
                duration_ms = duration,
                error = safe_err.as_str(),
                "Tool execution failed"
            );
            Err(e)
        }
    }
}

/// Helper to record success in current span
pub fn record_success() {
    Span::current().record("success", true);
    if should_sample() {
        info!("Operation completed successfully");
    }
    increment_log_count();
}

/// Helper to record failure in current span with error details
pub fn record_failure(error: &str) {
    let safe_err = redact_secrets(&sanitize_for_log(error));
    Span::current().record("success", false);
    Span::current().record("error", safe_err.as_str());
    error!(error = safe_err.as_str(), "Operation failed");
}

/// Span guard for agent loop steps
pub fn enter_agent_step(state: &str, step: usize) -> tracing::span::Span {
    let safe_state = sanitize_for_log(state);
    let span = info_span!("agent.step", state = safe_state.as_str(), step = step,);
    span
}

/// Record agent state transition
pub fn record_state_transition(from: &str, to: &str) {
    let safe_from = sanitize_for_log(from);
    let safe_to = sanitize_for_log(to);
    if should_sample() {
        info!(
            from = safe_from.as_str(),
            to = safe_to.as_str(),
            "Agent state transition"
        );
    }
    increment_log_count();
}

/// Initialize tracing for tests with a simple subscriber
#[cfg(test)]
pub fn init_test_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let result: Result<i32, &str> =
            track_tool_execution("test_tool", || async { Ok(42) }).await;

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
        let result: Result<bool, &str> =
            track_tool_execution("bool_tool", || async { Ok(true) }).await;

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
                track_tool_execution(&format!("sequential_tool_{}", i), || async move { Ok(i) })
                    .await;
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
        // Should not rotate when well below limit
        // (log entry count is shared across tests, but should be far below MAX_LOG_ENTRIES)
        let rotated = rotate_if_needed();
        // We can't guarantee state across tests, but if count < MAX_LOG_ENTRIES, should be false
        if log_entry_count() < MAX_LOG_ENTRIES {
            assert!(!rotated);
        }
    }

    #[test]
    fn test_max_log_entries_constant() {
        assert_eq!(MAX_LOG_ENTRIES, 100_000);
    }
}
