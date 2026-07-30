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

use metrics_exporter_prometheus::PrometheusBuilder;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use tracing::Instrument;
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

/// Guard for the non-blocking tracing writer's background thread.
/// Stored here instead of being leaked so it can be dropped for clean shutdown.
static TRACING_GUARD: OnceLock<Mutex<Option<tracing_appender::non_blocking::WorkerGuard>>> =
    OnceLock::new();

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
/// Redact sensitive data patterns from a string before logging.
///
/// Uses the canonical pattern suite in `safety::redact`, then collapses the
/// typed markers back to the plain `[REDACTED]` this layer's callers and
/// tests expect (the typed variants are for safety-check reporting).
pub fn redact_secrets(input: &str) -> String {
    let redacted = crate::safety::redact::redact_secrets(input).into_owned();
    static TYPED_MARKER: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    TYPED_MARKER
        .get_or_init(|| regex::Regex::new(r"\[REDACTED_[A-Z_]+\]").expect("marker regex"))
        .replace_all(&redacted, "[REDACTED]")
        .into_owned()
}

/// A [`tracing_subscriber::fmt::MakeWriter`] that redacts likely-secret
/// substrings (see [`redact_secrets`]) from every formatted log line before
/// it reaches the underlying writer.
///
/// This is applied to *both* the stderr and persistent-file log layers in
/// [`init_tracing_with_filter`] so redaction isn't something individual
/// `warn!`/`error!`/`debug!` call sites have to remember to do themselves --
/// a raw server error body echoing back an API key on an auth failure, for
/// example, is redacted here regardless of which call site logged it.
struct RedactingMakeWriter<M> {
    inner: M,
}

impl<M> RedactingMakeWriter<M> {
    fn new(inner: M) -> Self {
        Self { inner }
    }
}

impl<'a, M> tracing_subscriber::fmt::MakeWriter<'a> for RedactingMakeWriter<M>
where
    M: tracing_subscriber::fmt::MakeWriter<'a>,
{
    type Writer = RedactingWriter<M::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        RedactingWriter {
            inner: self.inner.make_writer(),
            buf: Vec::new(),
        }
    }
}

/// Buffers every `write()` call for a single formatted event (the fmt layer
/// calls `make_writer()` once per event, per its own documented contract, and
/// writes the event's fields via several small `write_str`/`write_fmt` calls
/// against that one instance) and redacts the *complete* line on flush/drop.
/// Redacting per-`write()`-call instead would risk missing a secret whose
/// bytes happen to fall across two of those small writes.
struct RedactingWriter<W: std::io::Write> {
    inner: W,
    buf: Vec<u8>,
}

impl<W: std::io::Write> RedactingWriter<W> {
    fn flush_redacted(&mut self) -> std::io::Result<()> {
        if !self.buf.is_empty() {
            let text = String::from_utf8_lossy(&self.buf);
            let redacted = redact_secrets(&text);
            self.inner.write_all(redacted.as_bytes())?;
            self.buf.clear();
        }
        self.inner.flush()
    }
}

impl<W: std::io::Write> std::io::Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.flush_redacted()
    }
}

impl<W: std::io::Write> Drop for RedactingWriter<W> {
    fn drop(&mut self) {
        let _ = self.flush_redacted();
    }
}

/// Initialize global tracing subscriber with configurable output
/// By default, only enables tracing if RUST_LOG is explicitly set
pub fn init_tracing() {
    // Initialize tracing if RUST_LOG or SELFWARE_LOG_LEVEL is set.
    // SELFWARE_LOG_LEVEL serves as a project-specific fallback.
    let filter = std::env::var("RUST_LOG").or_else(|_| std::env::var("SELFWARE_LOG_LEVEL"));
    if let Ok(f) = filter {
        init_tracing_with_filter(&f);
    }
}

/// Initialize tracing only for debug/verbose mode
pub fn init_tracing_verbose() {
    init_tracing_with_filter("info")
}

/// Initialize with custom filter string, file log rotation, and OpenTelemetry
pub fn init_tracing_with_filter(filter: &str) {
    // Skip if already initialized
    use std::sync::Once;
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        let filter_layer = EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("warn"));

        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_target(false)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_file(false)
            .with_line_number(false)
            .with_level(true)
            .compact()
            .with_writer(RedactingMakeWriter::new(std::io::stderr)); // Write to stderr, not stdout

        // Implement Log Rotation with daily rolling
        let log_dir = dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("selfware")
            .join("logs");
        let _ = std::fs::create_dir_all(&log_dir);
        let file_appender = tracing_appender::rolling::daily(log_dir, "selfware.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        // Store the guard so the background thread stays alive; drop via shutdown_tracing()
        let _ = TRACING_GUARD.set(Mutex::new(Some(guard)));

        let file_layer = tracing_subscriber::fmt::layer()
            .with_writer(RedactingMakeWriter::new(non_blocking))
            .with_ansi(false)
            .with_file(true)
            .with_line_number(true);

        // OpenTelemetry setup (if endpoint provided via env)
        let subscriber = tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .with(file_layer);

        if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
            use opentelemetry_otlp::WithExportConfig;
            if let Ok(tracer) = opentelemetry_otlp::new_pipeline()
                .tracing()
                .with_exporter(
                    opentelemetry_otlp::new_exporter()
                        .tonic()
                        .with_endpoint(endpoint),
                )
                .install_batch(opentelemetry_sdk::runtime::Tokio)
            {
                let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
                let _ = subscriber.with(telemetry).try_init();
                return; // Early return to avoid double init
            }
        }

        let _ = subscriber.try_init();
    });
}

/// Flush and shut down the tracing background writer.
/// Call this during graceful shutdown to ensure all logs are flushed.
pub fn shutdown_tracing() {
    if let Some(guard_slot) = TRACING_GUARD.get() {
        if let Ok(mut slot) = guard_slot.lock() {
            drop(slot.take()); // Drop the guard, flushing the writer
        }
    }
}

/// Application-wide metrics counters
pub struct Metrics {
    pub api_requests: AtomicU64,
    pub api_errors: AtomicU64,
    pub tool_executions: AtomicU64,
    pub tool_errors: AtomicU64,
    pub tokens_processed: AtomicU64,
}

static METRICS: Metrics = Metrics {
    api_requests: AtomicU64::new(0),
    api_errors: AtomicU64::new(0),
    tool_executions: AtomicU64::new(0),
    tool_errors: AtomicU64::new(0),
    tokens_processed: AtomicU64::new(0),
};

pub fn increment_api_requests() {
    METRICS.api_requests.fetch_add(1, Ordering::Relaxed);
    metrics::increment_counter!("selfware_api_requests_total");
}
pub fn increment_api_errors() {
    METRICS.api_errors.fetch_add(1, Ordering::Relaxed);
    metrics::increment_counter!("selfware_api_errors_total");
}
pub fn increment_tool_executions() {
    METRICS.tool_executions.fetch_add(1, Ordering::Relaxed);
    metrics::increment_counter!("selfware_tool_executions_total");
}
pub fn increment_tool_errors() {
    METRICS.tool_errors.fetch_add(1, Ordering::Relaxed);
    metrics::increment_counter!("selfware_tool_errors_total");
}
pub fn add_tokens_processed(count: u64) {
    METRICS.tokens_processed.fetch_add(count, Ordering::Relaxed);
    metrics::counter!("selfware_tokens_processed_total", count);
}
pub fn get_metrics() -> &'static Metrics {
    &METRICS
}

pub fn record_workflow_run(
    workflow_name: &str,
    status: &str,
    duration_ms: u64,
    llm_calls: u64,
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    estimated_cost_usd: f64,
) {
    metrics::counter!(
        "selfware_workflow_runs_total",
        1,
        "workflow" => workflow_name.to_string(),
        "status" => status.to_string()
    );
    metrics::histogram!(
        "selfware_workflow_duration_ms",
        duration_ms as f64,
        "workflow" => workflow_name.to_string(),
        "status" => status.to_string()
    );
    metrics::counter!(
        "selfware_workflow_llm_calls_total",
        llm_calls,
        "workflow" => workflow_name.to_string()
    );
    metrics::counter!(
        "selfware_workflow_prompt_tokens_total",
        prompt_tokens,
        "workflow" => workflow_name.to_string()
    );
    metrics::counter!(
        "selfware_workflow_completion_tokens_total",
        completion_tokens,
        "workflow" => workflow_name.to_string()
    );
    metrics::counter!(
        "selfware_workflow_total_tokens_total",
        total_tokens,
        "workflow" => workflow_name.to_string()
    );
    metrics::histogram!(
        "selfware_workflow_estimated_cost_usd",
        estimated_cost_usd,
        "workflow" => workflow_name.to_string()
    );
}

pub fn record_workflow_llm_call(
    workflow_name: &str,
    model: &str,
    latency_ms: u64,
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    estimated_cost_usd: f64,
) {
    metrics::counter!(
        "selfware_workflow_llm_requests_total",
        1,
        "workflow" => workflow_name.to_string(),
        "model" => model.to_string()
    );
    metrics::histogram!(
        "selfware_workflow_llm_latency_ms",
        latency_ms as f64,
        "workflow" => workflow_name.to_string(),
        "model" => model.to_string()
    );
    metrics::counter!(
        "selfware_workflow_prompt_tokens_total",
        prompt_tokens,
        "workflow" => workflow_name.to_string(),
        "model" => model.to_string()
    );
    metrics::counter!(
        "selfware_workflow_completion_tokens_total",
        completion_tokens,
        "workflow" => workflow_name.to_string(),
        "model" => model.to_string()
    );
    metrics::counter!(
        "selfware_workflow_total_tokens_total",
        total_tokens,
        "workflow" => workflow_name.to_string(),
        "model" => model.to_string()
    );
    metrics::histogram!(
        "selfware_workflow_llm_estimated_cost_usd",
        estimated_cost_usd,
        "workflow" => workflow_name.to_string(),
        "model" => model.to_string()
    );
}

// Guardrail telemetry
//
// The `swl_guardrail_checks_total` / `swl_guardrail_violations_total`
// counters are incremented directly by the guardrail enforcer
// (`swl/guardrails/enforcer.rs`) via `metrics::counter!` — deliberately NOT
// through helper functions here. Previous helper wrappers
// (`record_guardrail_check`, `record_guardrail_violation`, and duplicate
// incrementers) had no call sites, and their label-carrying series
// (`swl_guardrail_check_total`, `swl_guardrail_violation_total`) were
// described to Prometheus below but never incremented by any code path —
// empty exported series. They were removed rather than left as
// dead-but-advertised surface.

/// Start Prometheus Metrics Exporter (if in daemon mode).
///
/// Installs the `metrics-exporter-prometheus` global recorder and binds an
/// HTTP endpoint at `bind_addr` that serves metrics in Prometheus text format.
/// After installation, every call to `increment_api_requests()` etc. is
/// automatically captured and exported.
pub fn start_prometheus_exporter(bind_addr: std::net::SocketAddr) -> anyhow::Result<()> {
    PrometheusBuilder::new()
        .with_http_listener(bind_addr)
        .install()
        .map_err(|e| anyhow::anyhow!("Failed to start Prometheus exporter: {}", e))?;

    // Register metric descriptions so Prometheus shows HELP text.
    metrics::describe_counter!(
        "selfware_api_requests_total",
        "Total number of LLM API requests made"
    );
    metrics::describe_counter!(
        "selfware_api_errors_total",
        "Total number of LLM API errors"
    );
    metrics::describe_counter!(
        "selfware_tool_executions_total",
        "Total number of tool executions"
    );
    metrics::describe_counter!(
        "selfware_tool_errors_total",
        "Total number of tool execution errors"
    );
    metrics::describe_counter!(
        "selfware_tokens_processed_total",
        "Total number of tokens processed"
    );
    metrics::describe_counter!(
        "selfware_workflow_runs_total",
        "Total number of workflow executions"
    );
    metrics::describe_histogram!(
        "selfware_workflow_duration_ms",
        "Workflow execution duration in milliseconds"
    );
    metrics::describe_counter!(
        "selfware_workflow_llm_calls_total",
        "Total number of LLM calls made during workflow execution"
    );
    metrics::describe_counter!(
        "selfware_workflow_llm_requests_total",
        "Total number of workflow LLM requests"
    );
    metrics::describe_histogram!(
        "selfware_workflow_llm_latency_ms",
        "Workflow LLM request latency in milliseconds"
    );
    metrics::describe_counter!(
        "selfware_workflow_prompt_tokens_total",
        "Total prompt tokens consumed by workflows"
    );
    metrics::describe_counter!(
        "selfware_workflow_completion_tokens_total",
        "Total completion tokens generated by workflows"
    );
    metrics::describe_counter!(
        "selfware_workflow_total_tokens_total",
        "Total tokens consumed by workflows"
    );
    metrics::describe_histogram!(
        "selfware_workflow_estimated_cost_usd",
        "Estimated workflow execution cost in USD"
    );
    metrics::describe_histogram!(
        "selfware_workflow_llm_estimated_cost_usd",
        "Estimated workflow LLM request cost in USD"
    );

    // Guardrail metrics. Only the series that are actually incremented
    // (by `swl/guardrails/enforcer.rs`) are described — never-recorded
    // series must not be advertised with HELP text.
    metrics::describe_counter!(
        "swl_guardrail_checks_total",
        "Total number of guardrail checks performed"
    );
    metrics::describe_counter!(
        "swl_guardrail_violations_total",
        "Total number of guardrail violations detected"
    );

    Ok(())
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

    increment_tool_executions();

    async {
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
                increment_tool_errors();
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
    .instrument(span.clone())
    .await
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
#[path = "../../tests/unit/observability/telemetry/telemetry_test.rs"]
mod tests;
