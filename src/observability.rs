//! Observability Dashboard
//!
//! Analytics and telemetry:
//! - Token usage tracking
//! - Latency histograms
//! - Tool success rates
//! - Error tracking
//! - CLI stats command

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Token usage tracking
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input tokens
    pub input: usize,
    /// Output tokens
    pub output: usize,
    /// Total tokens
    pub total: usize,
    /// Estimated cost
    pub cost: Option<f64>,
}

impl TokenUsage {
    /// Create new usage
    pub fn new(input: usize, output: usize) -> Self {
        Self {
            input,
            output,
            total: input + output,
            cost: None,
        }
    }

    /// With cost calculation
    pub fn with_cost(mut self, input_rate: f64, output_rate: f64) -> Self {
        self.cost = Some(
            (self.input as f64 / 1000.0) * input_rate + (self.output as f64 / 1000.0) * output_rate,
        );
        self
    }

    /// Add more usage
    pub fn add(&mut self, other: &TokenUsage) {
        self.input += other.input;
        self.output += other.output;
        self.total += other.total;
        if let (Some(a), Some(b)) = (self.cost, other.cost) {
            self.cost = Some(a + b);
        }
    }

    /// Format for display
    pub fn display(&self) -> String {
        let cost = self
            .cost
            .map(|c| format!(" (${:.4})", c))
            .unwrap_or_default();
        format!(
            "{} tokens ({} in, {} out){}",
            self.total, self.input, self.output, cost
        )
    }
}

/// Token tracker for ongoing usage
#[derive(Debug, Default)]
pub struct TokenTracker {
    /// Session usage
    session: TokenUsage,
    /// By model
    by_model: HashMap<String, TokenUsage>,
    /// By hour
    by_hour: HashMap<String, TokenUsage>,
    /// Daily usage
    by_day: HashMap<String, TokenUsage>,
    /// Total all time
    all_time: TokenUsage,
}

impl TokenTracker {
    /// Create new tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Record usage
    pub fn record(&mut self, model: &str, usage: TokenUsage) {
        self.session.add(&usage);
        self.all_time.add(&usage);

        self.by_model
            .entry(model.to_string())
            .or_default()
            .add(&usage);

        let hour_key = Utc::now().format("%Y-%m-%d %H:00").to_string();
        self.by_hour.entry(hour_key).or_default().add(&usage);

        let day_key = Utc::now().format("%Y-%m-%d").to_string();
        self.by_day.entry(day_key).or_default().add(&usage);
    }

    /// Get session usage
    pub fn session(&self) -> &TokenUsage {
        &self.session
    }

    /// Get all time usage
    pub fn all_time(&self) -> &TokenUsage {
        &self.all_time
    }

    /// Get usage by model
    pub fn by_model(&self, model: &str) -> Option<&TokenUsage> {
        self.by_model.get(model)
    }

    /// Get today's usage
    pub fn today(&self) -> Option<&TokenUsage> {
        let key = Utc::now().format("%Y-%m-%d").to_string();
        self.by_day.get(&key)
    }

    /// Reset session
    pub fn reset_session(&mut self) {
        self.session = TokenUsage::default();
    }

    /// Get top models by usage
    pub fn top_models(&self, n: usize) -> Vec<(&String, &TokenUsage)> {
        let mut models: Vec<_> = self.by_model.iter().collect();
        models.sort_by(|a, b| b.1.total.cmp(&a.1.total));
        models.into_iter().take(n).collect()
    }
}

/// Latency measurement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyMeasurement {
    /// Duration
    pub duration: Duration,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Label (e.g., model name, tool name)
    pub label: String,
}

impl LatencyMeasurement {
    /// Create new measurement
    pub fn new(duration: Duration, label: String) -> Self {
        Self {
            duration,
            timestamp: Utc::now(),
            label,
        }
    }

    /// Duration in milliseconds
    pub fn ms(&self) -> i64 {
        self.duration.num_milliseconds()
    }
}

/// Histogram bucket
#[derive(Debug, Clone, Default)]
pub struct HistogramBucket {
    /// Bucket boundary (upper)
    pub boundary: i64,
    /// Count in this bucket
    pub count: usize,
}

/// Latency histogram
#[derive(Debug, Clone)]
pub struct LatencyHistogram {
    /// Label
    pub label: String,
    /// Measurements
    measurements: Vec<LatencyMeasurement>,
    /// Bucket boundaries (ms)
    boundaries: Vec<i64>,
    /// Maximum measurements to keep
    max_measurements: usize,
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new("default")
    }
}

impl LatencyHistogram {
    /// Create new histogram
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            measurements: Vec::new(),
            boundaries: vec![10, 50, 100, 250, 500, 1000, 2500, 5000, 10000],
            max_measurements: 1000,
        }
    }

    /// Set custom boundaries
    pub fn with_boundaries(mut self, boundaries: Vec<i64>) -> Self {
        self.boundaries = boundaries;
        self
    }

    /// Record a measurement
    pub fn record(&mut self, duration: Duration) {
        self.measurements
            .push(LatencyMeasurement::new(duration, self.label.clone()));

        // Limit size
        if self.measurements.len() > self.max_measurements {
            self.measurements.remove(0);
        }
    }

    /// Record with milliseconds
    pub fn record_ms(&mut self, ms: i64) {
        self.record(Duration::milliseconds(ms));
    }

    /// Get count
    pub fn count(&self) -> usize {
        self.measurements.len()
    }

    /// Get minimum latency
    pub fn min(&self) -> Option<Duration> {
        self.measurements.iter().map(|m| m.duration).min()
    }

    /// Get maximum latency
    pub fn max(&self) -> Option<Duration> {
        self.measurements.iter().map(|m| m.duration).max()
    }

    /// Get mean latency
    pub fn mean(&self) -> Option<Duration> {
        if self.measurements.is_empty() {
            return None;
        }
        let sum: i64 = self.measurements.iter().map(|m| m.ms()).sum();
        Some(Duration::milliseconds(sum / self.measurements.len() as i64))
    }

    /// Get median latency
    pub fn median(&self) -> Option<Duration> {
        if self.measurements.is_empty() {
            return None;
        }
        let mut values: Vec<i64> = self.measurements.iter().map(|m| m.ms()).collect();
        values.sort();
        let mid = values.len() / 2;
        Some(Duration::milliseconds(values[mid]))
    }

    /// Get percentile
    pub fn percentile(&self, p: f64) -> Option<Duration> {
        if self.measurements.is_empty() {
            return None;
        }
        let mut values: Vec<i64> = self.measurements.iter().map(|m| m.ms()).collect();
        values.sort();
        let idx = ((p / 100.0) * (values.len() - 1) as f64) as usize;
        Some(Duration::milliseconds(values[idx]))
    }

    /// Get p50
    pub fn p50(&self) -> Option<Duration> {
        self.percentile(50.0)
    }

    /// Get p90
    pub fn p90(&self) -> Option<Duration> {
        self.percentile(90.0)
    }

    /// Get p99
    pub fn p99(&self) -> Option<Duration> {
        self.percentile(99.0)
    }

    /// Get bucket counts
    pub fn buckets(&self) -> Vec<HistogramBucket> {
        let mut buckets: Vec<HistogramBucket> = self
            .boundaries
            .iter()
            .map(|&b| HistogramBucket {
                boundary: b,
                count: 0,
            })
            .collect();

        // Add overflow bucket
        buckets.push(HistogramBucket {
            boundary: i64::MAX,
            count: 0,
        });

        for m in &self.measurements {
            let ms = m.ms();
            for bucket in &mut buckets {
                if ms <= bucket.boundary {
                    bucket.count += 1;
                    break;
                }
            }
        }

        buckets
    }

    /// Render histogram as ASCII
    pub fn render(&self, width: usize) -> String {
        let buckets = self.buckets();
        let max_count = buckets.iter().map(|b| b.count).max().unwrap_or(1);

        let mut lines = Vec::new();
        for (i, bucket) in buckets.iter().enumerate() {
            let label = if bucket.boundary == i64::MAX {
                ">10s".to_string()
            } else {
                format!("≤{}ms", bucket.boundary)
            };

            let bar_width = if max_count > 0 {
                (bucket.count * width) / max_count
            } else {
                0
            };
            let bar = "█".repeat(bar_width);

            // Skip if previous boundary covers this
            if i > 0 && bucket.count == 0 {
                continue;
            }

            lines.push(format!("{:>7} {} {} ", label, bar, bucket.count));
        }

        lines.join("\n")
    }

    /// Summary statistics
    pub fn summary(&self) -> String {
        format!(
            "count={} min={:?} mean={:?} p50={:?} p90={:?} p99={:?} max={:?}",
            self.count(),
            self.min(),
            self.mean(),
            self.p50(),
            self.p90(),
            self.p99(),
            self.max()
        )
    }

    /// Clear measurements
    pub fn clear(&mut self) {
        self.measurements.clear();
    }
}

/// Tool execution result
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolResult {
    Success,
    Failure,
    Timeout,
    Skipped,
}

impl ToolResult {
    /// Is successful
    pub fn is_success(&self) -> bool {
        matches!(self, ToolResult::Success)
    }

    /// Icon
    pub fn icon(&self) -> &'static str {
        match self {
            ToolResult::Success => "✓",
            ToolResult::Failure => "✗",
            ToolResult::Timeout => "⏱",
            ToolResult::Skipped => "⊘",
        }
    }
}

/// Tool execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecution {
    /// Tool name
    pub tool: String,
    /// Result
    pub result: ToolResult,
    /// Duration
    pub duration: Duration,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Error message if failed
    pub error: Option<String>,
}

impl ToolExecution {
    /// Create new execution
    pub fn new(tool: &str, result: ToolResult, duration: Duration) -> Self {
        Self {
            tool: tool.to_string(),
            result,
            duration,
            timestamp: Utc::now(),
            error: None,
        }
    }

    /// With error
    pub fn with_error(mut self, error: String) -> Self {
        self.error = Some(error);
        self
    }
}

/// Tool success tracker
#[derive(Debug, Default)]
pub struct ToolTracker {
    /// Executions by tool
    executions: HashMap<String, Vec<ToolExecution>>,
    /// Total counts
    total_success: usize,
    total_failure: usize,
    total_timeout: usize,
}

impl ToolTracker {
    /// Create new tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an execution
    pub fn record(&mut self, execution: ToolExecution) {
        match execution.result {
            ToolResult::Success => self.total_success += 1,
            ToolResult::Failure => self.total_failure += 1,
            ToolResult::Timeout => self.total_timeout += 1,
            ToolResult::Skipped => {}
        }

        self.executions
            .entry(execution.tool.clone())
            .or_default()
            .push(execution);
    }

    /// Get success rate for a tool
    pub fn success_rate(&self, tool: &str) -> Option<f64> {
        let execs = self.executions.get(tool)?;
        if execs.is_empty() {
            return None;
        }
        let success = execs.iter().filter(|e| e.result.is_success()).count();
        Some((success as f64 / execs.len() as f64) * 100.0)
    }

    /// Get overall success rate
    pub fn overall_success_rate(&self) -> f64 {
        let total = self.total_success + self.total_failure + self.total_timeout;
        if total == 0 {
            return 100.0;
        }
        (self.total_success as f64 / total as f64) * 100.0
    }

    /// Get execution count for a tool
    pub fn execution_count(&self, tool: &str) -> usize {
        self.executions.get(tool).map(|e| e.len()).unwrap_or(0)
    }

    /// Get total executions
    pub fn total_executions(&self) -> usize {
        self.total_success + self.total_failure + self.total_timeout
    }

    /// Get tools sorted by usage
    pub fn tools_by_usage(&self) -> Vec<(&String, usize)> {
        let mut tools: Vec<_> = self
            .executions
            .iter()
            .map(|(name, execs)| (name, execs.len()))
            .collect();
        tools.sort_by(|a, b| b.1.cmp(&a.1));
        tools
    }

    /// Get tools sorted by success rate
    pub fn tools_by_success_rate(&self) -> Vec<(&String, f64)> {
        let mut tools: Vec<_> = self
            .executions
            .keys()
            .filter_map(|name| self.success_rate(name).map(|rate| (name, rate)))
            .collect();
        tools.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        tools
    }

    /// Get recent failures
    pub fn recent_failures(&self, limit: usize) -> Vec<&ToolExecution> {
        let mut failures: Vec<_> = self
            .executions
            .values()
            .flatten()
            .filter(|e| !e.result.is_success())
            .collect();
        failures.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        failures.into_iter().take(limit).collect()
    }

    /// Get summary
    pub fn summary(&self) -> ToolTrackerSummary {
        ToolTrackerSummary {
            total: self.total_executions(),
            success: self.total_success,
            failure: self.total_failure,
            timeout: self.total_timeout,
            success_rate: self.overall_success_rate(),
            tool_count: self.executions.len(),
        }
    }
}

/// Tool tracker summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolTrackerSummary {
    pub total: usize,
    pub success: usize,
    pub failure: usize,
    pub timeout: usize,
    pub success_rate: f64,
    pub tool_count: usize,
}

impl ToolTrackerSummary {
    /// Display
    pub fn display(&self) -> String {
        format!(
            "{} tools, {} total ({} success, {} failure, {} timeout) - {:.1}% success rate",
            self.tool_count,
            self.total,
            self.success,
            self.failure,
            self.timeout,
            self.success_rate
        )
    }
}

/// Error severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorLevel {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

impl ErrorLevel {
    /// Icon
    pub fn icon(&self) -> &'static str {
        match self {
            ErrorLevel::Debug => "🔍",
            ErrorLevel::Info => "ℹ️",
            ErrorLevel::Warning => "⚠️",
            ErrorLevel::Error => "❌",
            ErrorLevel::Critical => "🔥",
        }
    }

    /// Color code
    pub fn color(&self) -> &'static str {
        match self {
            ErrorLevel::Debug => "\x1b[90m",
            ErrorLevel::Info => "\x1b[34m",
            ErrorLevel::Warning => "\x1b[33m",
            ErrorLevel::Error => "\x1b[31m",
            ErrorLevel::Critical => "\x1b[91m",
        }
    }
}

impl std::fmt::Display for ErrorLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            ErrorLevel::Debug => "DEBUG",
            ErrorLevel::Info => "INFO",
            ErrorLevel::Warning => "WARN",
            ErrorLevel::Error => "ERROR",
            ErrorLevel::Critical => "CRITICAL",
        };
        write!(f, "{}", name)
    }
}

/// Error record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRecord {
    /// Error level
    pub level: ErrorLevel,
    /// Error message
    pub message: String,
    /// Error code/type
    pub code: Option<String>,
    /// Context/location
    pub context: Option<String>,
    /// Stack trace
    pub stack_trace: Option<String>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Is resolved
    pub resolved: bool,
}

impl ErrorRecord {
    /// Create new error
    pub fn new(level: ErrorLevel, message: String) -> Self {
        Self {
            level,
            message,
            code: None,
            context: None,
            stack_trace: None,
            timestamp: Utc::now(),
            resolved: false,
        }
    }

    /// With code
    pub fn with_code(mut self, code: &str) -> Self {
        self.code = Some(code.to_string());
        self
    }

    /// With context
    pub fn with_context(mut self, context: &str) -> Self {
        self.context = Some(context.to_string());
        self
    }

    /// With stack trace
    pub fn with_stack_trace(mut self, trace: String) -> Self {
        self.stack_trace = Some(trace);
        self
    }

    /// Mark resolved
    pub fn resolve(&mut self) {
        self.resolved = true;
    }

    /// Display
    pub fn display(&self) -> String {
        let resolved = if self.resolved { " (resolved)" } else { "" };
        format!(
            "[{}] {}{}: {}",
            self.timestamp.format("%Y-%m-%d %H:%M:%S"),
            self.level,
            resolved,
            self.message
        )
    }
}

/// Error tracker
#[derive(Debug, Default)]
pub struct ErrorTracker {
    /// All errors
    errors: Vec<ErrorRecord>,
    /// Count by level
    by_level: HashMap<ErrorLevel, usize>,
    /// Count by code
    by_code: HashMap<String, usize>,
    /// Max errors to keep
    max_errors: usize,
}

impl ErrorTracker {
    /// Create new tracker
    pub fn new() -> Self {
        Self {
            max_errors: 1000,
            ..Default::default()
        }
    }

    /// Record an error
    pub fn record(&mut self, error: ErrorRecord) {
        *self.by_level.entry(error.level).or_insert(0) += 1;
        if let Some(code) = &error.code {
            *self.by_code.entry(code.clone()).or_insert(0) += 1;
        }

        self.errors.push(error);

        // Limit size
        if self.errors.len() > self.max_errors {
            self.errors.remove(0);
        }
    }

    /// Get error count
    pub fn count(&self) -> usize {
        self.errors.len()
    }

    /// Get count by level
    pub fn count_by_level(&self, level: ErrorLevel) -> usize {
        *self.by_level.get(&level).unwrap_or(&0)
    }

    /// Get recent errors
    pub fn recent(&self, limit: usize) -> Vec<&ErrorRecord> {
        self.errors.iter().rev().take(limit).collect()
    }

    /// Get unresolved errors
    pub fn unresolved(&self) -> Vec<&ErrorRecord> {
        self.errors.iter().filter(|e| !e.resolved).collect()
    }

    /// Get errors by level
    pub fn by_level(&self, level: ErrorLevel) -> Vec<&ErrorRecord> {
        self.errors.iter().filter(|e| e.level == level).collect()
    }

    /// Get critical errors
    pub fn critical(&self) -> Vec<&ErrorRecord> {
        self.by_level(ErrorLevel::Critical)
    }

    /// Clear all errors
    pub fn clear(&mut self) {
        self.errors.clear();
        self.by_level.clear();
        self.by_code.clear();
    }

    /// Summary
    pub fn summary(&self) -> ErrorTrackerSummary {
        ErrorTrackerSummary {
            total: self.errors.len(),
            unresolved: self.unresolved().len(),
            critical: self.count_by_level(ErrorLevel::Critical),
            error: self.count_by_level(ErrorLevel::Error),
            warning: self.count_by_level(ErrorLevel::Warning),
        }
    }
}

/// Error tracker summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorTrackerSummary {
    pub total: usize,
    pub unresolved: usize,
    pub critical: usize,
    pub error: usize,
    pub warning: usize,
}

impl ErrorTrackerSummary {
    /// Display
    pub fn display(&self) -> String {
        format!(
            "{} errors ({} unresolved): {} critical, {} error, {} warning",
            self.total, self.unresolved, self.critical, self.error, self.warning
        )
    }
}

/// Session statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionStats {
    /// Session start time
    pub started_at: Option<DateTime<Utc>>,
    /// Session end time
    pub ended_at: Option<DateTime<Utc>>,
    /// Total requests
    pub requests: usize,
    /// Total tokens
    pub tokens: TokenUsage,
    /// Tool executions
    pub tool_executions: usize,
    /// Errors encountered
    pub errors: usize,
}

impl SessionStats {
    /// Create new stats
    pub fn new() -> Self {
        Self {
            started_at: Some(Utc::now()),
            ..Default::default()
        }
    }

    /// Session duration
    pub fn duration(&self) -> Option<Duration> {
        let start = self.started_at?;
        let end = self.ended_at.unwrap_or_else(Utc::now);
        Some(end - start)
    }

    /// Requests per minute
    pub fn requests_per_minute(&self) -> Option<f64> {
        let duration = self.duration()?;
        let minutes = duration.num_seconds() as f64 / 60.0;
        if minutes > 0.0 {
            Some(self.requests as f64 / minutes)
        } else {
            None
        }
    }

    /// End session
    pub fn end(&mut self) {
        self.ended_at = Some(Utc::now());
    }

    /// Display
    pub fn display(&self) -> String {
        let duration = self
            .duration()
            .map(|d| format!("{:.1}min", d.num_seconds() as f64 / 60.0))
            .unwrap_or("ongoing".to_string());

        format!(
            "Session ({}): {} requests, {}, {} tool calls, {} errors",
            duration,
            self.requests,
            self.tokens.display(),
            self.tool_executions,
            self.errors
        )
    }
}

/// Complete observability dashboard
#[derive(Debug, Default)]
pub struct ObservabilityDashboard {
    /// Token tracker
    pub tokens: TokenTracker,
    /// Latency histograms by label
    pub latency: HashMap<String, LatencyHistogram>,
    /// Tool tracker
    pub tools: ToolTracker,
    /// Error tracker
    pub errors: ErrorTracker,
    /// Session stats
    pub session: SessionStats,
}

impl ObservabilityDashboard {
    /// Create new dashboard
    pub fn new() -> Self {
        Self {
            session: SessionStats::new(),
            ..Default::default()
        }
    }

    /// Record API request
    pub fn record_request(&mut self, model: &str, tokens: TokenUsage, latency: Duration) {
        self.tokens.record(model, tokens.clone());
        self.session.tokens.add(&tokens);
        self.session.requests += 1;

        self.latency
            .entry(model.to_string())
            .or_insert_with(|| LatencyHistogram::new(model))
            .record(latency);
    }

    /// Record tool execution
    pub fn record_tool(&mut self, execution: ToolExecution) {
        self.session.tool_executions += 1;
        self.tools.record(execution);
    }

    /// Record error
    pub fn record_error(&mut self, error: ErrorRecord) {
        self.session.errors += 1;
        self.errors.record(error);
    }

    /// Get latency histogram for a label
    pub fn get_latency(&self, label: &str) -> Option<&LatencyHistogram> {
        self.latency.get(label)
    }

    /// End session
    pub fn end_session(&mut self) {
        self.session.end();
    }

    /// Generate stats report
    pub fn stats_report(&self) -> StatsReport {
        StatsReport {
            session: self.session.clone(),
            token_summary: self.tokens.session().clone(),
            tool_summary: self.tools.summary(),
            error_summary: self.errors.summary(),
        }
    }
}

/// Stats report for CLI output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsReport {
    pub session: SessionStats,
    pub token_summary: TokenUsage,
    pub tool_summary: ToolTrackerSummary,
    pub error_summary: ErrorTrackerSummary,
}

impl StatsReport {
    /// Render as text
    pub fn render(&self) -> String {
        let mut lines = Vec::new();

        lines.push("═══════════════════════════════════════".to_string());
        lines.push("         OBSERVABILITY REPORT          ".to_string());
        lines.push("═══════════════════════════════════════".to_string());
        lines.push(String::new());

        lines.push("Session:".to_string());
        lines.push(format!("  {}", self.session.display()));
        lines.push(String::new());

        lines.push("Tokens:".to_string());
        lines.push(format!("  {}", self.token_summary.display()));
        lines.push(String::new());

        lines.push("Tools:".to_string());
        lines.push(format!("  {}", self.tool_summary.display()));
        lines.push(String::new());

        lines.push("Errors:".to_string());
        lines.push(format!("  {}", self.error_summary.display()));

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_usage_new() {
        let usage = TokenUsage::new(100, 50);
        assert_eq!(usage.input, 100);
        assert_eq!(usage.output, 50);
        assert_eq!(usage.total, 150);
    }

    #[test]
    fn test_token_usage_with_cost() {
        let usage = TokenUsage::new(1000, 1000).with_cost(0.01, 0.03);
        assert!(usage.cost.is_some());
        assert_eq!(usage.cost.unwrap(), 0.04);
    }

    #[test]
    fn test_token_usage_add() {
        let mut usage1 = TokenUsage::new(100, 50);
        let usage2 = TokenUsage::new(200, 100);
        usage1.add(&usage2);

        assert_eq!(usage1.input, 300);
        assert_eq!(usage1.output, 150);
        assert_eq!(usage1.total, 450);
    }

    #[test]
    fn test_token_usage_display() {
        let usage = TokenUsage::new(100, 50);
        let display = usage.display();
        assert!(display.contains("150"));
        assert!(display.contains("100 in"));
        assert!(display.contains("50 out"));
    }

    #[test]
    fn test_token_tracker_new() {
        let tracker = TokenTracker::new();
        assert_eq!(tracker.session().total, 0);
    }

    #[test]
    fn test_token_tracker_record() {
        let mut tracker = TokenTracker::new();
        tracker.record("gpt-4", TokenUsage::new(100, 50));

        assert_eq!(tracker.session().total, 150);
        assert!(tracker.by_model("gpt-4").is_some());
    }

    #[test]
    fn test_token_tracker_reset_session() {
        let mut tracker = TokenTracker::new();
        tracker.record("gpt-4", TokenUsage::new(100, 50));
        tracker.reset_session();

        assert_eq!(tracker.session().total, 0);
        assert_eq!(tracker.all_time().total, 150); // All time is preserved
    }

    #[test]
    fn test_token_tracker_top_models() {
        let mut tracker = TokenTracker::new();
        tracker.record("gpt-4", TokenUsage::new(500, 200));
        tracker.record("claude", TokenUsage::new(1000, 500));
        tracker.record("gpt-4", TokenUsage::new(100, 50));

        let top = tracker.top_models(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, "claude"); // Claude has more tokens
    }

    #[test]
    fn test_latency_measurement() {
        let m = LatencyMeasurement::new(Duration::milliseconds(100), "test".to_string());
        assert_eq!(m.ms(), 100);
    }

    #[test]
    fn test_latency_histogram_new() {
        let h = LatencyHistogram::new("test");
        assert_eq!(h.count(), 0);
    }

    #[test]
    fn test_latency_histogram_record() {
        let mut h = LatencyHistogram::new("test");
        h.record(Duration::milliseconds(100));
        h.record(Duration::milliseconds(200));

        assert_eq!(h.count(), 2);
    }

    #[test]
    fn test_latency_histogram_stats() {
        let mut h = LatencyHistogram::new("test");
        h.record_ms(100);
        h.record_ms(200);
        h.record_ms(300);

        assert_eq!(h.min(), Some(Duration::milliseconds(100)));
        assert_eq!(h.max(), Some(Duration::milliseconds(300)));
        assert_eq!(h.mean(), Some(Duration::milliseconds(200)));
    }

    #[test]
    fn test_latency_histogram_percentiles() {
        let mut h = LatencyHistogram::new("test");
        for i in 1..=100 {
            h.record_ms(i);
        }

        assert!(h.p50().is_some());
        assert!(h.p90().is_some());
        assert!(h.p99().is_some());
    }

    #[test]
    fn test_latency_histogram_buckets() {
        let mut h = LatencyHistogram::new("test");
        h.record_ms(5); // ≤10
        h.record_ms(75); // ≤100
        h.record_ms(200); // ≤250

        let buckets = h.buckets();
        assert!(!buckets.is_empty());
    }

    #[test]
    fn test_latency_histogram_render() {
        let mut h = LatencyHistogram::new("test");
        h.record_ms(100);
        h.record_ms(200);

        let render = h.render(20);
        assert!(!render.is_empty());
    }

    #[test]
    fn test_latency_histogram_clear() {
        let mut h = LatencyHistogram::new("test");
        h.record_ms(100);
        h.clear();
        assert_eq!(h.count(), 0);
    }

    #[test]
    fn test_tool_result_is_success() {
        assert!(ToolResult::Success.is_success());
        assert!(!ToolResult::Failure.is_success());
        assert!(!ToolResult::Timeout.is_success());
    }

    #[test]
    fn test_tool_result_icon() {
        assert_eq!(ToolResult::Success.icon(), "✓");
        assert_eq!(ToolResult::Failure.icon(), "✗");
    }

    #[test]
    fn test_tool_execution_new() {
        let exec = ToolExecution::new("file_read", ToolResult::Success, Duration::milliseconds(50));
        assert_eq!(exec.tool, "file_read");
        assert!(exec.result.is_success());
    }

    #[test]
    fn test_tool_execution_with_error() {
        let exec = ToolExecution::new("file_read", ToolResult::Failure, Duration::milliseconds(50))
            .with_error("File not found".to_string());
        assert!(exec.error.is_some());
    }

    #[test]
    fn test_tool_tracker_new() {
        let tracker = ToolTracker::new();
        assert_eq!(tracker.total_executions(), 0);
    }

    #[test]
    fn test_tool_tracker_record() {
        let mut tracker = ToolTracker::new();
        tracker.record(ToolExecution::new(
            "file_read",
            ToolResult::Success,
            Duration::milliseconds(50),
        ));

        assert_eq!(tracker.total_executions(), 1);
        assert_eq!(tracker.execution_count("file_read"), 1);
    }

    #[test]
    fn test_tool_tracker_success_rate() {
        let mut tracker = ToolTracker::new();
        tracker.record(ToolExecution::new(
            "tool",
            ToolResult::Success,
            Duration::milliseconds(50),
        ));
        tracker.record(ToolExecution::new(
            "tool",
            ToolResult::Success,
            Duration::milliseconds(50),
        ));
        tracker.record(ToolExecution::new(
            "tool",
            ToolResult::Failure,
            Duration::milliseconds(50),
        ));

        let rate = tracker.success_rate("tool").unwrap();
        assert!((rate - 66.66).abs() < 1.0);
    }

    #[test]
    fn test_tool_tracker_overall_success_rate() {
        let mut tracker = ToolTracker::new();
        tracker.record(ToolExecution::new(
            "a",
            ToolResult::Success,
            Duration::milliseconds(50),
        ));
        tracker.record(ToolExecution::new(
            "b",
            ToolResult::Failure,
            Duration::milliseconds(50),
        ));

        assert_eq!(tracker.overall_success_rate(), 50.0);
    }

    #[test]
    fn test_tool_tracker_tools_by_usage() {
        let mut tracker = ToolTracker::new();
        tracker.record(ToolExecution::new(
            "a",
            ToolResult::Success,
            Duration::milliseconds(50),
        ));
        tracker.record(ToolExecution::new(
            "b",
            ToolResult::Success,
            Duration::milliseconds(50),
        ));
        tracker.record(ToolExecution::new(
            "b",
            ToolResult::Success,
            Duration::milliseconds(50),
        ));

        let by_usage = tracker.tools_by_usage();
        assert_eq!(by_usage[0].0, "b");
    }

    #[test]
    fn test_tool_tracker_recent_failures() {
        let mut tracker = ToolTracker::new();
        tracker.record(ToolExecution::new(
            "a",
            ToolResult::Failure,
            Duration::milliseconds(50),
        ));
        tracker.record(ToolExecution::new(
            "b",
            ToolResult::Success,
            Duration::milliseconds(50),
        ));

        let failures = tracker.recent_failures(10);
        assert_eq!(failures.len(), 1);
    }

    #[test]
    fn test_tool_tracker_summary() {
        let mut tracker = ToolTracker::new();
        tracker.record(ToolExecution::new(
            "a",
            ToolResult::Success,
            Duration::milliseconds(50),
        ));

        let summary = tracker.summary();
        assert_eq!(summary.total, 1);
        assert_eq!(summary.success, 1);
    }

    #[test]
    fn test_error_level_icon() {
        assert_eq!(ErrorLevel::Debug.icon(), "🔍");
        assert_eq!(ErrorLevel::Critical.icon(), "🔥");
    }

    #[test]
    fn test_error_level_display() {
        assert_eq!(format!("{}", ErrorLevel::Error), "ERROR");
        assert_eq!(format!("{}", ErrorLevel::Warning), "WARN");
    }

    #[test]
    fn test_error_record_new() {
        let error = ErrorRecord::new(ErrorLevel::Error, "Something went wrong".to_string());
        assert_eq!(error.level, ErrorLevel::Error);
        assert!(!error.resolved);
    }

    #[test]
    fn test_error_record_builder() {
        let error = ErrorRecord::new(ErrorLevel::Error, "Error".to_string())
            .with_code("E001")
            .with_context("main.rs:42");

        assert_eq!(error.code, Some("E001".to_string()));
        assert_eq!(error.context, Some("main.rs:42".to_string()));
    }

    #[test]
    fn test_error_record_resolve() {
        let mut error = ErrorRecord::new(ErrorLevel::Error, "Error".to_string());
        error.resolve();
        assert!(error.resolved);
    }

    #[test]
    fn test_error_tracker_new() {
        let tracker = ErrorTracker::new();
        assert_eq!(tracker.count(), 0);
    }

    #[test]
    fn test_error_tracker_record() {
        let mut tracker = ErrorTracker::new();
        tracker.record(ErrorRecord::new(ErrorLevel::Error, "Error".to_string()));

        assert_eq!(tracker.count(), 1);
        assert_eq!(tracker.count_by_level(ErrorLevel::Error), 1);
    }

    #[test]
    fn test_error_tracker_unresolved() {
        let mut tracker = ErrorTracker::new();
        let mut error = ErrorRecord::new(ErrorLevel::Error, "Error".to_string());
        tracker.record(error.clone());

        error.resolve();
        tracker.record(error);

        assert_eq!(tracker.unresolved().len(), 1);
    }

    #[test]
    fn test_error_tracker_by_level() {
        let mut tracker = ErrorTracker::new();
        tracker.record(ErrorRecord::new(ErrorLevel::Error, "Error".to_string()));
        tracker.record(ErrorRecord::new(ErrorLevel::Warning, "Warning".to_string()));

        assert_eq!(tracker.by_level(ErrorLevel::Error).len(), 1);
    }

    #[test]
    fn test_error_tracker_critical() {
        let mut tracker = ErrorTracker::new();
        tracker.record(ErrorRecord::new(
            ErrorLevel::Critical,
            "Critical".to_string(),
        ));

        assert_eq!(tracker.critical().len(), 1);
    }

    #[test]
    fn test_error_tracker_clear() {
        let mut tracker = ErrorTracker::new();
        tracker.record(ErrorRecord::new(ErrorLevel::Error, "Error".to_string()));
        tracker.clear();

        assert_eq!(tracker.count(), 0);
    }

    #[test]
    fn test_session_stats_new() {
        let stats = SessionStats::new();
        assert!(stats.started_at.is_some());
        assert_eq!(stats.requests, 0);
    }

    #[test]
    fn test_session_stats_duration() {
        let stats = SessionStats::new();
        assert!(stats.duration().is_some());
    }

    #[test]
    fn test_session_stats_end() {
        let mut stats = SessionStats::new();
        stats.end();
        assert!(stats.ended_at.is_some());
    }

    #[test]
    fn test_observability_dashboard_new() {
        let dashboard = ObservabilityDashboard::new();
        assert!(dashboard.session.started_at.is_some());
    }

    #[test]
    fn test_observability_dashboard_record_request() {
        let mut dashboard = ObservabilityDashboard::new();
        dashboard.record_request(
            "gpt-4",
            TokenUsage::new(100, 50),
            Duration::milliseconds(200),
        );

        assert_eq!(dashboard.session.requests, 1);
        assert_eq!(dashboard.tokens.session().total, 150);
    }

    #[test]
    fn test_observability_dashboard_record_tool() {
        let mut dashboard = ObservabilityDashboard::new();
        dashboard.record_tool(ToolExecution::new(
            "file_read",
            ToolResult::Success,
            Duration::milliseconds(50),
        ));

        assert_eq!(dashboard.session.tool_executions, 1);
    }

    #[test]
    fn test_observability_dashboard_record_error() {
        let mut dashboard = ObservabilityDashboard::new();
        dashboard.record_error(ErrorRecord::new(ErrorLevel::Error, "Error".to_string()));

        assert_eq!(dashboard.session.errors, 1);
    }

    #[test]
    fn test_observability_dashboard_stats_report() {
        let mut dashboard = ObservabilityDashboard::new();
        dashboard.record_request(
            "gpt-4",
            TokenUsage::new(100, 50),
            Duration::milliseconds(200),
        );

        let report = dashboard.stats_report();
        assert_eq!(report.session.requests, 1);
    }

    #[test]
    fn test_stats_report_render() {
        let report = StatsReport {
            session: SessionStats::new(),
            token_summary: TokenUsage::new(100, 50),
            tool_summary: ToolTrackerSummary {
                total: 10,
                success: 8,
                failure: 2,
                timeout: 0,
                success_rate: 80.0,
                tool_count: 3,
            },
            error_summary: ErrorTrackerSummary {
                total: 1,
                unresolved: 1,
                critical: 0,
                error: 1,
                warning: 0,
            },
        };

        let render = report.render();
        assert!(render.contains("OBSERVABILITY REPORT"));
        assert!(render.contains("Session"));
        assert!(render.contains("Tokens"));
    }
}
