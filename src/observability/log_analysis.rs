//! Log Analysis System
//!
//! This module provides comprehensive log analysis:
//! - Pattern detection in log streams
//! - Anomaly identification
//! - Root cause analysis
//! - Alert correlation
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Log Analyzer                             │
//! │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐   │
//! │  │ Log           │  │ Pattern       │  │ Anomaly       │   │
//! │  │ Parser        │  │ Detector      │  │ Detector      │   │
//! │  └───────────────┘  └───────────────┘  └───────────────┘   │
//! │           │                  │                  │           │
//! │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐   │
//! │  │ Root Cause    │  │ Alert         │  │ Statistics    │   │
//! │  │ Analyzer      │  │ Correlator    │  │ Tracker       │   │
//! │  └───────────────┘  └───────────────┘  └───────────────┘   │
//! └─────────────────────────────────────────────────────────────┘
//! ```

#![allow(dead_code, unused_imports, unused_variables)]

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

// ============================================================================
// Log Entry
// ============================================================================

/// Log severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl LogLevel {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "TRACE" => LogLevel::Trace,
            "DEBUG" => LogLevel::Debug,
            "INFO" => LogLevel::Info,
            "WARN" | "WARNING" => LogLevel::Warn,
            "ERROR" | "ERR" => LogLevel::Error,
            "FATAL" | "CRITICAL" | "CRIT" => LogLevel::Fatal,
            _ => LogLevel::Info,
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self, LogLevel::Error | LogLevel::Fatal)
    }
}

/// Parsed log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Unique ID
    pub id: u64,
    /// Timestamp
    pub timestamp: u64,
    /// Log level
    pub level: LogLevel,
    /// Source/component
    pub source: String,
    /// Message
    pub message: String,
    /// Structured fields
    pub fields: HashMap<String, String>,
    /// Raw log line
    pub raw: String,
}

impl LogEntry {
    pub fn new(level: LogLevel, source: &str, message: &str) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            id: now * 1000 + (now % 1000),
            timestamp: now,
            level,
            source: source.to_string(),
            message: message.to_string(),
            fields: HashMap::new(),
            raw: String::new(),
        }
    }

    pub fn with_field(mut self, key: &str, value: &str) -> Self {
        self.fields.insert(key.to_string(), value.to_string());
        self
    }

    pub fn with_raw(mut self, raw: &str) -> Self {
        self.raw = raw.to_string();
        self
    }
}

// ============================================================================
// Log Parser
// ============================================================================

/// Log format type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogFormat {
    /// Plain text logs
    Plain,
    /// JSON structured logs
    Json,
    /// Common Log Format (Apache)
    CommonLog,
    /// Syslog format
    Syslog,
    /// Custom regex pattern
    Custom,
}

/// Log parser
pub struct LogParser {
    format: LogFormat,
    /// Custom patterns for extracting fields
    patterns: Vec<(String, regex::Regex)>,
}

impl LogParser {
    pub fn new(format: LogFormat) -> Self {
        Self {
            format,
            patterns: Vec::new(),
        }
    }

    /// Add a custom pattern
    pub fn add_pattern(&mut self, name: &str, pattern: &str) {
        if let Ok(regex) = regex::Regex::new(pattern) {
            self.patterns.push((name.to_string(), regex));
        }
    }

    /// Parse a log line
    pub fn parse(&self, line: &str) -> Option<LogEntry> {
        match self.format {
            LogFormat::Json => self.parse_json(line),
            LogFormat::Plain => self.parse_plain(line),
            LogFormat::CommonLog => self.parse_common(line),
            LogFormat::Syslog => self.parse_syslog(line),
            LogFormat::Custom => self.parse_custom(line),
        }
    }

    fn parse_json(&self, line: &str) -> Option<LogEntry> {
        let json: serde_json::Value = serde_json::from_str(line).ok()?;

        let level = json
            .get("level")
            .or_else(|| json.get("severity"))
            .and_then(|v| v.as_str())
            .map(LogLevel::from_str)
            .unwrap_or(LogLevel::Info);

        let message = json
            .get("message")
            .or_else(|| json.get("msg"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let source = json
            .get("source")
            .or_else(|| json.get("component"))
            .or_else(|| json.get("logger"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let timestamp = json
            .get("timestamp")
            .or_else(|| json.get("time"))
            .or_else(|| json.get("ts"))
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            });

        let mut fields = HashMap::new();
        if let Some(obj) = json.as_object() {
            for (k, v) in obj {
                if ![
                    "level",
                    "severity",
                    "message",
                    "msg",
                    "source",
                    "timestamp",
                    "time",
                ]
                .contains(&k.as_str())
                {
                    fields.insert(k.clone(), v.to_string());
                }
            }
        }

        Some(LogEntry {
            id: timestamp * 1000 + (timestamp % 1000),
            timestamp,
            level,
            source,
            message,
            fields,
            raw: line.to_string(),
        })
    }

    fn parse_plain(&self, line: &str) -> Option<LogEntry> {
        // Try to parse: [LEVEL] [SOURCE] Message
        // or: TIMESTAMP LEVEL SOURCE: Message

        let level = if line.contains("[ERROR]") || line.contains(" ERROR ") {
            LogLevel::Error
        } else if line.contains("[WARN]") || line.contains(" WARN ") {
            LogLevel::Warn
        } else if line.contains("[DEBUG]") || line.contains(" DEBUG ") {
            LogLevel::Debug
        } else if line.contains("[INFO]") || line.contains(" INFO ") {
            LogLevel::Info
        } else if line.contains("[FATAL]") || line.contains(" FATAL ") {
            LogLevel::Fatal
        } else {
            LogLevel::Info
        };

        Some(LogEntry::new(level, "unknown", line).with_raw(line))
    }

    fn parse_common(&self, line: &str) -> Option<LogEntry> {
        // Common Log Format: host ident authuser [date] "request" status bytes
        let parts: Vec<&str> = line.splitn(6, ' ').collect();
        if parts.len() < 5 {
            return self.parse_plain(line);
        }

        let mut fields = HashMap::new();
        fields.insert("host".to_string(), parts[0].to_string());

        Some(LogEntry {
            id: 0,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            level: LogLevel::Info,
            source: "httpd".to_string(),
            message: line.to_string(),
            fields,
            raw: line.to_string(),
        })
    }

    fn parse_syslog(&self, line: &str) -> Option<LogEntry> {
        // Syslog: <priority>timestamp hostname app[pid]: message
        let parts: Vec<&str> = line.splitn(4, ' ').collect();
        if parts.len() < 4 {
            return self.parse_plain(line);
        }

        let source = parts.get(2).unwrap_or(&"unknown").to_string();
        let message = parts.get(3).unwrap_or(&"").to_string();

        Some(LogEntry::new(LogLevel::Info, &source, &message).with_raw(line))
    }

    fn parse_custom(&self, line: &str) -> Option<LogEntry> {
        let mut fields = HashMap::new();

        for (name, pattern) in &self.patterns {
            if let Some(captures) = pattern.captures(line) {
                if let Some(m) = captures.get(1) {
                    fields.insert(name.clone(), m.as_str().to_string());
                }
            }
        }

        let level = fields
            .get("level")
            .map(|s| LogLevel::from_str(s))
            .unwrap_or(LogLevel::Info);

        let message = fields
            .get("message")
            .cloned()
            .unwrap_or_else(|| line.to_string());

        Some(LogEntry {
            id: 0,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            level,
            source: fields
                .get("source")
                .cloned()
                .unwrap_or_else(|| "unknown".to_string()),
            message,
            fields,
            raw: line.to_string(),
        })
    }
}

impl Default for LogParser {
    fn default() -> Self {
        Self::new(LogFormat::Plain)
    }
}

// ============================================================================
// Pattern Detection
// ============================================================================

/// Detected log pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogPattern {
    /// Pattern ID
    pub id: String,
    /// Pattern template (with placeholders)
    pub template: String,
    /// Occurrence count
    pub count: u64,
    /// First seen timestamp
    pub first_seen: u64,
    /// Last seen timestamp
    pub last_seen: u64,
    /// Example log entries
    pub examples: Vec<String>,
    /// Severity level
    pub level: LogLevel,
}

/// Pattern detector
pub struct PatternDetector {
    /// Detected patterns
    patterns: RwLock<HashMap<String, LogPattern>>,
    /// Similarity threshold
    threshold: f32,
    /// Statistics
    stats: PatternStats,
}

/// Pattern detection statistics
#[derive(Debug, Default)]
pub struct PatternStats {
    pub logs_processed: AtomicU64,
    pub patterns_detected: AtomicU64,
    pub pattern_matches: AtomicU64,
}

impl PatternDetector {
    pub fn new(threshold: f32) -> Self {
        Self {
            patterns: RwLock::new(HashMap::new()),
            threshold: threshold.clamp(0.0, 1.0),
            stats: PatternStats::default(),
        }
    }

    /// Process a log entry
    pub fn process(&self, entry: &LogEntry) {
        self.stats.logs_processed.fetch_add(1, Ordering::Relaxed);

        let template = self.extract_template(&entry.message);
        let pattern_id = self.hash_template(&template);

        if let Ok(mut patterns) = self.patterns.write() {
            if let Some(pattern) = patterns.get_mut(&pattern_id) {
                pattern.count += 1;
                pattern.last_seen = entry.timestamp;
                if pattern.examples.len() < 3 {
                    pattern.examples.push(entry.message.clone());
                }
                self.stats.pattern_matches.fetch_add(1, Ordering::Relaxed);
            } else {
                let pattern = LogPattern {
                    id: pattern_id.clone(),
                    template: template.clone(),
                    count: 1,
                    first_seen: entry.timestamp,
                    last_seen: entry.timestamp,
                    examples: vec![entry.message.clone()],
                    level: entry.level,
                };
                patterns.insert(pattern_id, pattern);
                self.stats.patterns_detected.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Extract template from message (replace variable parts)
    fn extract_template(&self, message: &str) -> String {
        use std::sync::LazyLock;

        static NUM_RE: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"\d+").expect("invalid number regex"));
        static UUID_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
            regex::Regex::new(
                r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
            )
            .expect("invalid UUID regex")
        });
        static IP_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
            regex::Regex::new(r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}").expect("invalid IP regex")
        });
        static PATH_RE: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"/[\w/.-]+").expect("invalid path regex"));

        let mut template = message.to_string();

        // Replace numbers
        template = NUM_RE.replace_all(&template, "<NUM>").to_string();

        // Replace UUIDs
        template = UUID_RE.replace_all(&template, "<UUID>").to_string();

        // Replace IP addresses
        template = IP_RE.replace_all(&template, "<IP>").to_string();

        // Replace paths
        template = PATH_RE.replace_all(&template, "<PATH>").to_string();

        template
    }

    /// Hash template for identification
    fn hash_template(&self, template: &str) -> String {
        let mut hash: u64 = 5381;
        for byte in template.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
        }
        format!("pat_{:x}", hash)
    }

    /// Get the similarity threshold
    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    /// Get top patterns
    pub fn top_patterns(&self, n: usize) -> Vec<LogPattern> {
        self.patterns
            .read()
            .map(|p| {
                let mut patterns: Vec<_> = p.values().cloned().collect();
                patterns.sort_by_key(|x| std::cmp::Reverse(x.count));
                patterns.truncate(n);
                patterns
            })
            .unwrap_or_default()
    }

    /// Get error patterns
    pub fn error_patterns(&self) -> Vec<LogPattern> {
        self.patterns
            .read()
            .map(|p| {
                p.values()
                    .filter(|pat| pat.level.is_error())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get summary
    pub fn summary(&self) -> PatternSummary {
        PatternSummary {
            logs_processed: self.stats.logs_processed.load(Ordering::Relaxed),
            patterns_detected: self.stats.patterns_detected.load(Ordering::Relaxed),
            pattern_matches: self.stats.pattern_matches.load(Ordering::Relaxed),
            unique_patterns: self.patterns.read().map(|p| p.len()).unwrap_or(0),
        }
    }

    /// Clear patterns
    pub fn clear(&self) {
        if let Ok(mut patterns) = self.patterns.write() {
            patterns.clear();
        }
    }
}

impl Default for PatternDetector {
    fn default() -> Self {
        Self::new(0.8)
    }
}

/// Pattern detection summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternSummary {
    pub logs_processed: u64,
    pub patterns_detected: u64,
    pub pattern_matches: u64,
    pub unique_patterns: usize,
}

// ============================================================================
// Anomaly Detection
// ============================================================================

/// Anomaly type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnomalyType {
    /// Sudden spike in errors
    ErrorSpike,
    /// Unusual pattern frequency
    FrequencyAnomaly,
    /// New error pattern
    NewError,
    /// Missing expected logs
    MissingLogs,
    /// Unusual source
    UnusualSource,
    /// Timing anomaly
    TimingAnomaly,
}

/// Detected anomaly
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anomaly {
    /// Anomaly ID
    pub id: String,
    /// Type
    pub anomaly_type: AnomalyType,
    /// Severity (0.0 - 1.0)
    pub severity: f32,
    /// Description
    pub description: String,
    /// Timestamp
    pub timestamp: u64,
    /// Related log entries
    pub related_logs: Vec<u64>,
    /// Suggested action
    pub suggested_action: Option<String>,
}

impl Anomaly {
    pub fn new(anomaly_type: AnomalyType, severity: f32, description: &str) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            id: format!("anom_{}", now),
            anomaly_type,
            severity: severity.clamp(0.0, 1.0),
            description: description.to_string(),
            timestamp: now,
            related_logs: Vec::new(),
            suggested_action: None,
        }
    }

    pub fn with_related_logs(mut self, logs: Vec<u64>) -> Self {
        self.related_logs = logs;
        self
    }

    pub fn with_action(mut self, action: &str) -> Self {
        self.suggested_action = Some(action.to_string());
        self
    }
}

/// Anomaly detector
pub struct AnomalyDetector {
    /// Error rate baseline
    error_baseline: RwLock<f32>,
    /// Recent error counts by window
    error_counts: RwLock<VecDeque<(u64, u32)>>,
    /// Known sources
    known_sources: RwLock<Vec<String>>,
    /// Detected anomalies
    anomalies: RwLock<VecDeque<Anomaly>>,
    /// Statistics
    stats: AnomalyStats,
}

/// Anomaly detection statistics
#[derive(Debug, Default)]
pub struct AnomalyStats {
    pub logs_analyzed: AtomicU64,
    pub anomalies_detected: AtomicU64,
}

impl AnomalyDetector {
    pub fn new() -> Self {
        Self {
            error_baseline: RwLock::new(0.05), // 5% baseline error rate
            error_counts: RwLock::new(VecDeque::with_capacity(60)),
            known_sources: RwLock::new(Vec::new()),
            anomalies: RwLock::new(VecDeque::with_capacity(100)),
            stats: AnomalyStats::default(),
        }
    }

    /// Analyze a log entry
    pub fn analyze(&self, entry: &LogEntry) -> Option<Anomaly> {
        self.stats.logs_analyzed.fetch_add(1, Ordering::Relaxed);

        // Check for unusual source
        if let Some(anomaly) = self.check_unusual_source(entry) {
            return Some(anomaly);
        }

        // Check for error spike
        if entry.level.is_error() {
            if let Some(anomaly) = self.check_error_spike(entry) {
                return Some(anomaly);
            }
        }

        None
    }

    fn check_unusual_source(&self, entry: &LogEntry) -> Option<Anomaly> {
        let mut is_new = false;

        if let Ok(mut sources) = self.known_sources.write() {
            if !sources.contains(&entry.source) {
                sources.push(entry.source.clone());
                is_new = sources.len() > 10; // Only flag as unusual after baseline
            }
        }

        if is_new {
            let anomaly = Anomaly::new(
                AnomalyType::UnusualSource,
                0.6,
                &format!("New log source detected: {}", entry.source),
            )
            .with_related_logs(vec![entry.id]);

            self.record_anomaly(anomaly.clone());
            return Some(anomaly);
        }

        None
    }

    fn check_error_spike(&self, entry: &LogEntry) -> Option<Anomaly> {
        let window = entry.timestamp / 60; // 1-minute windows

        if let Ok(mut counts) = self.error_counts.write() {
            // Find or create window entry
            if let Some(last) = counts.back_mut() {
                if last.0 == window {
                    last.1 += 1;
                } else {
                    counts.push_back((window, 1));
                }
            } else {
                counts.push_back((window, 1));
            }

            // Keep only last 60 windows
            while counts.len() > 60 {
                counts.pop_front();
            }

            // Check for spike
            if counts.len() >= 5 {
                let recent: Vec<_> = counts.iter().rev().take(5).collect();
                let avg: f32 =
                    counts.iter().map(|(_, c)| *c as f32).sum::<f32>() / counts.len() as f32;

                if let Some(current) = recent.first() {
                    // Use error_baseline to determine if current rate is anomalous
                    let baseline = self.error_baseline.read().map(|b| *b).unwrap_or(0.05);
                    let spike_threshold = (avg * 3.0).max(baseline * 100.0);
                    if current.1 as f32 > spike_threshold && current.1 > 5 {
                        let anomaly = Anomaly::new(
                            AnomalyType::ErrorSpike,
                            0.8,
                            &format!(
                                "Error spike detected: {} errors in 1 minute (avg: {:.1})",
                                current.1, avg
                            ),
                        )
                        .with_action("Investigate recent changes and check system health");

                        self.record_anomaly(anomaly.clone());
                        return Some(anomaly);
                    }
                }
            }
        }

        None
    }

    fn record_anomaly(&self, anomaly: Anomaly) {
        self.stats
            .anomalies_detected
            .fetch_add(1, Ordering::Relaxed);

        if let Ok(mut anomalies) = self.anomalies.write() {
            anomalies.push_back(anomaly);
            while anomalies.len() > 100 {
                anomalies.pop_front();
            }
        }
    }

    /// Get the current error baseline
    pub fn error_baseline(&self) -> f32 {
        self.error_baseline.read().map(|b| *b).unwrap_or(0.05)
    }

    /// Get recent anomalies
    pub fn recent_anomalies(&self, count: usize) -> Vec<Anomaly> {
        self.anomalies
            .read()
            .map(|a| a.iter().rev().take(count).cloned().collect())
            .unwrap_or_default()
    }

    /// Get summary
    pub fn summary(&self) -> AnomalySummary {
        AnomalySummary {
            logs_analyzed: self.stats.logs_analyzed.load(Ordering::Relaxed),
            anomalies_detected: self.stats.anomalies_detected.load(Ordering::Relaxed),
            known_sources: self.known_sources.read().map(|s| s.len()).unwrap_or(0),
        }
    }

    /// Clear state
    pub fn clear(&self) {
        if let Ok(mut counts) = self.error_counts.write() {
            counts.clear();
        }
        if let Ok(mut anomalies) = self.anomalies.write() {
            anomalies.clear();
        }
    }
}

impl Default for AnomalyDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Anomaly detection summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalySummary {
    pub logs_analyzed: u64,
    pub anomalies_detected: u64,
    pub known_sources: usize,
}

// ============================================================================
// Root Cause Analysis
// ============================================================================

/// Root cause hypothesis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootCause {
    /// Cause ID
    pub id: String,
    /// Description
    pub description: String,
    /// Confidence (0.0 - 1.0)
    pub confidence: f32,
    /// Evidence (log IDs)
    pub evidence: Vec<u64>,
    /// Category
    pub category: RootCauseCategory,
    /// Suggested fix
    pub suggested_fix: Option<String>,
}

/// Root cause categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootCauseCategory {
    Configuration,
    Resource,
    Network,
    Dependency,
    Code,
    Data,
    Unknown,
}

/// Root cause analyzer
pub struct RootCauseAnalyzer {
    /// Analysis rules
    rules: Vec<AnalysisRule>,
    /// Recent analyses
    analyses: RwLock<VecDeque<RootCause>>,
}

/// Analysis rule
pub struct AnalysisRule {
    /// Rule name
    pub name: String,
    /// Pattern to match
    pub pattern: regex::Regex,
    /// Category
    pub category: RootCauseCategory,
    /// Description template
    pub description: String,
    /// Fix suggestion
    pub fix: String,
}

impl RootCauseAnalyzer {
    pub fn new() -> Self {
        let mut analyzer = Self {
            rules: Vec::new(),
            analyses: RwLock::new(VecDeque::with_capacity(50)),
        };

        // Add default rules
        analyzer.add_default_rules();
        analyzer
    }

    fn add_default_rules(&mut self) {
        // Connection refused
        if let Ok(pattern) = regex::Regex::new(r"(?i)connection refused|ECONNREFUSED") {
            self.rules.push(AnalysisRule {
                name: "connection_refused".to_string(),
                pattern,
                category: RootCauseCategory::Network,
                description: "Service connection refused - target service may be down".to_string(),
                fix: "Check if target service is running and accessible".to_string(),
            });
        }

        // Out of memory
        if let Ok(pattern) = regex::Regex::new(r"(?i)out of memory|OOM|memory exhausted") {
            self.rules.push(AnalysisRule {
                name: "out_of_memory".to_string(),
                pattern,
                category: RootCauseCategory::Resource,
                description: "Memory exhaustion detected".to_string(),
                fix: "Increase memory limits or optimize memory usage".to_string(),
            });
        }

        // Timeout
        if let Ok(pattern) = regex::Regex::new(r"(?i)timeout|timed out|deadline exceeded") {
            self.rules.push(AnalysisRule {
                name: "timeout".to_string(),
                pattern,
                category: RootCauseCategory::Network,
                description: "Operation timed out".to_string(),
                fix: "Check network latency and increase timeout if needed".to_string(),
            });
        }

        // Permission denied
        if let Ok(pattern) =
            regex::Regex::new(r"(?i)permission denied|access denied|forbidden|EACCES")
        {
            self.rules.push(AnalysisRule {
                name: "permission_denied".to_string(),
                pattern,
                category: RootCauseCategory::Configuration,
                description: "Permission/access issue".to_string(),
                fix: "Check file permissions and access credentials".to_string(),
            });
        }

        // Disk full
        if let Ok(pattern) = regex::Regex::new(r"(?i)no space left|disk full|ENOSPC") {
            self.rules.push(AnalysisRule {
                name: "disk_full".to_string(),
                pattern,
                category: RootCauseCategory::Resource,
                description: "Disk space exhausted".to_string(),
                fix: "Free up disk space or add more storage".to_string(),
            });
        }
    }

    /// Analyze log entries for root cause
    pub fn analyze(&self, entries: &[LogEntry]) -> Vec<RootCause> {
        let mut causes = Vec::new();

        for entry in entries.iter().filter(|e| e.level.is_error()) {
            for rule in &self.rules {
                if rule.pattern.is_match(&entry.message) {
                    let cause = RootCause {
                        id: format!("rc_{}_{}", rule.name, entry.id),
                        description: rule.description.clone(),
                        confidence: 0.8,
                        evidence: vec![entry.id],
                        category: rule.category,
                        suggested_fix: Some(rule.fix.clone()),
                    };
                    causes.push(cause);
                }
            }
        }

        // Deduplicate by category
        let mut unique: HashMap<String, RootCause> = HashMap::new();
        for cause in causes {
            let key = format!("{:?}", cause.category);
            if let Some(existing) = unique.get_mut(&key) {
                existing.evidence.extend(cause.evidence);
                existing.confidence = (existing.confidence + cause.confidence) / 2.0;
            } else {
                unique.insert(key, cause);
            }
        }

        let result: Vec<_> = unique.into_values().collect();

        // Store analyses
        if let Ok(mut analyses) = self.analyses.write() {
            for cause in &result {
                analyses.push_back(cause.clone());
            }
            while analyses.len() > 50 {
                analyses.pop_front();
            }
        }

        result
    }

    /// Get recent analyses
    pub fn recent_analyses(&self, count: usize) -> Vec<RootCause> {
        self.analyses
            .read()
            .map(|a| a.iter().rev().take(count).cloned().collect())
            .unwrap_or_default()
    }
}

impl Default for RootCauseAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Alert Correlation
// ============================================================================

/// Alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// Alert ID
    pub id: String,
    /// Title
    pub title: String,
    /// Severity
    pub severity: AlertSeverity,
    /// Source
    pub source: String,
    /// Timestamp
    pub timestamp: u64,
    /// Related log IDs
    pub related_logs: Vec<u64>,
    /// Related alerts
    pub related_alerts: Vec<String>,
    /// Status
    pub status: AlertStatus,
}

/// Alert severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AlertSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Alert status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertStatus {
    Open,
    Acknowledged,
    Resolved,
    Suppressed,
}

impl Alert {
    pub fn new(title: &str, severity: AlertSeverity, source: &str) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            id: format!("alert_{}", now),
            title: title.to_string(),
            severity,
            source: source.to_string(),
            timestamp: now,
            related_logs: Vec::new(),
            related_alerts: Vec::new(),
            status: AlertStatus::Open,
        }
    }
}

/// Alert correlator
pub struct AlertCorrelator {
    /// Active alerts
    alerts: RwLock<HashMap<String, Alert>>,
    /// Correlation window (seconds)
    window_secs: u64,
    /// Statistics
    stats: CorrelatorStats,
}

/// Correlator statistics
#[derive(Debug, Default)]
pub struct CorrelatorStats {
    pub alerts_created: AtomicU64,
    pub alerts_correlated: AtomicU64,
    pub alerts_resolved: AtomicU64,
}

impl AlertCorrelator {
    pub fn new(window_secs: u64) -> Self {
        Self {
            alerts: RwLock::new(HashMap::new()),
            window_secs,
            stats: CorrelatorStats::default(),
        }
    }

    /// Create or correlate an alert
    pub fn process(
        &self,
        title: &str,
        severity: AlertSeverity,
        source: &str,
        log_ids: Vec<u64>,
    ) -> Alert {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Look for existing correlated alert
        if let Ok(mut alerts) = self.alerts.write() {
            // Find alerts from same source in correlation window
            let related: Vec<String> = alerts
                .iter()
                .filter(|(_, a)| {
                    a.source == source
                        && a.status == AlertStatus::Open
                        && now - a.timestamp < self.window_secs
                })
                .map(|(id, _)| id.clone())
                .collect();

            if !related.is_empty() {
                // Correlate with existing
                if let Some(existing) = alerts.get_mut(&related[0]) {
                    existing.related_logs.extend(log_ids);
                    existing.related_alerts.extend(related[1..].to_vec());
                    self.stats.alerts_correlated.fetch_add(1, Ordering::Relaxed);
                    return existing.clone();
                }
            }

            // Create new alert
            let mut alert = Alert::new(title, severity, source);
            alert.related_logs = log_ids;
            alert.related_alerts = related;

            alerts.insert(alert.id.clone(), alert.clone());
            self.stats.alerts_created.fetch_add(1, Ordering::Relaxed);

            alert
        } else {
            Alert::new(title, severity, source)
        }
    }

    /// Resolve an alert
    pub fn resolve(&self, alert_id: &str) {
        if let Ok(mut alerts) = self.alerts.write() {
            if let Some(alert) = alerts.get_mut(alert_id) {
                alert.status = AlertStatus::Resolved;
                self.stats.alerts_resolved.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Get open alerts
    pub fn open_alerts(&self) -> Vec<Alert> {
        self.alerts
            .read()
            .map(|a| {
                a.values()
                    .filter(|alert| alert.status == AlertStatus::Open)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get summary
    pub fn summary(&self) -> CorrelatorSummary {
        CorrelatorSummary {
            alerts_created: self.stats.alerts_created.load(Ordering::Relaxed),
            alerts_correlated: self.stats.alerts_correlated.load(Ordering::Relaxed),
            alerts_resolved: self.stats.alerts_resolved.load(Ordering::Relaxed),
            open_alerts: self
                .alerts
                .read()
                .map(|a| {
                    a.values()
                        .filter(|al| al.status == AlertStatus::Open)
                        .count()
                })
                .unwrap_or(0),
        }
    }
}

impl Default for AlertCorrelator {
    fn default() -> Self {
        Self::new(300) // 5 minute correlation window
    }
}

/// Correlator summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelatorSummary {
    pub alerts_created: u64,
    pub alerts_correlated: u64,
    pub alerts_resolved: u64,
    pub open_alerts: usize,
}

// ============================================================================
// Log Analyzer (Unified)
// ============================================================================

/// Unified log analyzer
pub struct LogAnalyzer {
    /// Log parser
    parser: LogParser,
    /// Pattern detector
    patterns: PatternDetector,
    /// Anomaly detector
    anomalies: AnomalyDetector,
    /// Root cause analyzer
    root_cause: RootCauseAnalyzer,
    /// Alert correlator
    alerts: AlertCorrelator,
    /// Recent logs
    logs: RwLock<VecDeque<LogEntry>>,
}

impl LogAnalyzer {
    pub fn new(format: LogFormat) -> Self {
        Self {
            parser: LogParser::new(format),
            patterns: PatternDetector::default(),
            anomalies: AnomalyDetector::default(),
            root_cause: RootCauseAnalyzer::default(),
            alerts: AlertCorrelator::default(),
            logs: RwLock::new(VecDeque::with_capacity(10000)),
        }
    }

    /// Process a log line
    pub fn process_line(&self, line: &str) -> Option<LogEntry> {
        let entry = self.parser.parse(line)?;
        self.process_entry(entry.clone());
        Some(entry)
    }

    /// Process a log entry
    pub fn process_entry(&self, entry: LogEntry) {
        // Store log
        if let Ok(mut logs) = self.logs.write() {
            logs.push_back(entry.clone());
            while logs.len() > 10000 {
                logs.pop_front();
            }
        }

        // Detect patterns
        self.patterns.process(&entry);

        // Detect anomalies
        if let Some(anomaly) = self.anomalies.analyze(&entry) {
            // Create alert for high severity anomalies
            if anomaly.severity >= 0.7 {
                self.alerts.process(
                    &anomaly.description,
                    AlertSeverity::High,
                    &entry.source,
                    anomaly.related_logs.clone(),
                );
            }
        }
    }

    /// Analyze recent logs for root cause
    pub fn analyze_root_cause(&self) -> Vec<RootCause> {
        let logs: Vec<_> = self
            .logs
            .read()
            .map(|l| l.iter().cloned().collect())
            .unwrap_or_default();
        self.root_cause.analyze(&logs)
    }

    /// Get components
    pub fn patterns(&self) -> &PatternDetector {
        &self.patterns
    }

    pub fn anomalies(&self) -> &AnomalyDetector {
        &self.anomalies
    }

    pub fn alerts(&self) -> &AlertCorrelator {
        &self.alerts
    }

    /// Get comprehensive summary
    pub fn summary(&self) -> LogAnalyzerSummary {
        LogAnalyzerSummary {
            logs_stored: self.logs.read().map(|l| l.len()).unwrap_or(0),
            patterns: self.patterns.summary(),
            anomalies: self.anomalies.summary(),
            alerts: self.alerts.summary(),
        }
    }
}

impl Default for LogAnalyzer {
    fn default() -> Self {
        Self::new(LogFormat::Plain)
    }
}

/// Log analyzer summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogAnalyzerSummary {
    pub logs_stored: usize,
    pub patterns: PatternSummary,
    pub anomalies: AnomalySummary,
    pub alerts: CorrelatorSummary,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[path = "../../tests/unit/observability/log_analysis/log_analysis_test.rs"]
mod tests;
