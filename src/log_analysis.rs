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

#![allow(dead_code)]

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
        let mut template = message.to_string();

        // Replace numbers
        template = regex::Regex::new(r"\d+")
            .map(|re| re.replace_all(&template, "<NUM>").to_string())
            .unwrap_or(template);

        // Replace UUIDs
        template = regex::Regex::new(
            r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
        )
        .map(|re| re.replace_all(&template, "<UUID>").to_string())
        .unwrap_or(template);

        // Replace IP addresses
        template = regex::Regex::new(r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}")
            .map(|re| re.replace_all(&template, "<IP>").to_string())
            .unwrap_or(template);

        // Replace paths
        template = regex::Regex::new(r"/[\w/.-]+")
            .map(|re| re.replace_all(&template, "<PATH>").to_string())
            .unwrap_or(template);

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

    /// Get top patterns
    pub fn top_patterns(&self, n: usize) -> Vec<LogPattern> {
        self.patterns
            .read()
            .map(|p| {
                let mut patterns: Vec<_> = p.values().cloned().collect();
                patterns.sort_by(|a, b| b.count.cmp(&a.count));
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
                    if current.1 as f32 > avg * 3.0 && current.1 > 5 {
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
mod tests {
    use super::*;

    #[test]
    fn test_log_level_from_str() {
        assert_eq!(LogLevel::from_str("ERROR"), LogLevel::Error);
        assert_eq!(LogLevel::from_str("warn"), LogLevel::Warn);
        assert_eq!(LogLevel::from_str("INFO"), LogLevel::Info);
    }

    #[test]
    fn test_log_level_is_error() {
        assert!(LogLevel::Error.is_error());
        assert!(LogLevel::Fatal.is_error());
        assert!(!LogLevel::Warn.is_error());
    }

    #[test]
    fn test_log_entry_new() {
        let entry = LogEntry::new(LogLevel::Info, "app", "Test message");
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.source, "app");
        assert_eq!(entry.message, "Test message");
    }

    #[test]
    fn test_log_entry_with_field() {
        let entry = LogEntry::new(LogLevel::Info, "app", "msg").with_field("key", "value");
        assert_eq!(entry.fields.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_log_parser_json() {
        let parser = LogParser::new(LogFormat::Json);
        let line = r#"{"level":"error","message":"Test error","source":"app"}"#;

        let entry = parser.parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.message, "Test error");
    }

    #[test]
    fn test_log_parser_plain() {
        let parser = LogParser::new(LogFormat::Plain);
        let line = "[ERROR] Something went wrong";

        let entry = parser.parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Error);
    }

    #[test]
    fn test_pattern_detector_process() {
        let detector = PatternDetector::default();

        for _ in 0..5 {
            let entry = LogEntry::new(LogLevel::Error, "app", "Connection failed to 192.168.1.1");
            detector.process(&entry);
        }

        let summary = detector.summary();
        assert!(summary.logs_processed >= 5);
    }

    #[test]
    fn test_pattern_detector_top_patterns() {
        let detector = PatternDetector::default();

        for i in 0..10 {
            let entry = LogEntry::new(LogLevel::Info, "app", &format!("Request {} processed", i));
            detector.process(&entry);
        }

        let top = detector.top_patterns(5);
        assert!(!top.is_empty());
    }

    #[test]
    fn test_anomaly_new() {
        let anomaly = Anomaly::new(AnomalyType::ErrorSpike, 0.8, "Test anomaly");
        assert_eq!(anomaly.anomaly_type, AnomalyType::ErrorSpike);
        assert_eq!(anomaly.severity, 0.8);
    }

    #[test]
    fn test_anomaly_detector_analyze() {
        let detector = AnomalyDetector::default();
        let entry = LogEntry::new(LogLevel::Error, "new-source", "Error message");

        // First entry from new source after baseline shouldn't trigger
        let _result = detector.analyze(&entry);

        let summary = detector.summary();
        assert_eq!(summary.logs_analyzed, 1);
    }

    #[test]
    fn test_root_cause_analyzer() {
        let analyzer = RootCauseAnalyzer::default();

        let entries = vec![LogEntry::new(
            LogLevel::Error,
            "app",
            "Connection refused to localhost:5432",
        )];

        let causes = analyzer.analyze(&entries);
        assert!(!causes.is_empty());
        assert_eq!(causes[0].category, RootCauseCategory::Network);
    }

    #[test]
    fn test_alert_new() {
        let alert = Alert::new("Test alert", AlertSeverity::High, "app");
        assert_eq!(alert.title, "Test alert");
        assert_eq!(alert.severity, AlertSeverity::High);
        assert_eq!(alert.status, AlertStatus::Open);
    }

    #[test]
    fn test_alert_correlator_process() {
        let correlator = AlertCorrelator::default();

        let alert = correlator.process("Error 1", AlertSeverity::High, "app", vec![1, 2]);
        assert_eq!(alert.title, "Error 1");

        let summary = correlator.summary();
        assert_eq!(summary.alerts_created, 1);
    }

    #[test]
    fn test_alert_correlator_resolve() {
        let correlator = AlertCorrelator::default();
        let alert = correlator.process("Error", AlertSeverity::High, "app", vec![]);

        correlator.resolve(&alert.id);

        let open = correlator.open_alerts();
        assert!(open.is_empty());
    }

    #[test]
    fn test_log_analyzer_process_line() {
        let analyzer = LogAnalyzer::new(LogFormat::Plain);
        let entry = analyzer.process_line("[ERROR] Test error message");

        assert!(entry.is_some());
        assert_eq!(entry.unwrap().level, LogLevel::Error);
    }

    #[test]
    fn test_log_analyzer_summary() {
        let analyzer = LogAnalyzer::default();
        analyzer.process_line("[INFO] Test message");

        let summary = analyzer.summary();
        assert_eq!(summary.logs_stored, 1);
    }

    #[test]
    fn test_anomaly_type_enum() {
        assert_eq!(AnomalyType::ErrorSpike, AnomalyType::ErrorSpike);
        assert_ne!(AnomalyType::ErrorSpike, AnomalyType::NewError);
    }

    #[test]
    fn test_alert_severity_ordering() {
        assert!(AlertSeverity::Critical > AlertSeverity::High);
        assert!(AlertSeverity::High > AlertSeverity::Medium);
        assert!(AlertSeverity::Medium > AlertSeverity::Low);
    }

    // Additional tests for comprehensive coverage

    #[test]
    fn test_log_level_all_variants() {
        assert_eq!(LogLevel::from_str("TRACE"), LogLevel::Trace);
        assert_eq!(LogLevel::from_str("DEBUG"), LogLevel::Debug);
        assert_eq!(LogLevel::from_str("WARNING"), LogLevel::Warn);
        assert_eq!(LogLevel::from_str("ERR"), LogLevel::Error);
        assert_eq!(LogLevel::from_str("CRITICAL"), LogLevel::Fatal);
        assert_eq!(LogLevel::from_str("CRIT"), LogLevel::Fatal);
        assert_eq!(LogLevel::from_str("unknown"), LogLevel::Info);
    }

    #[test]
    fn test_log_level_clone_debug() {
        let level = LogLevel::Error;
        let cloned = level.clone();
        assert_eq!(level, cloned);
        let debug_str = format!("{:?}", level);
        assert!(debug_str.contains("Error"));
    }

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Fatal > LogLevel::Error);
        assert!(LogLevel::Error > LogLevel::Warn);
        assert!(LogLevel::Warn > LogLevel::Info);
        assert!(LogLevel::Info > LogLevel::Debug);
        assert!(LogLevel::Debug > LogLevel::Trace);
    }

    #[test]
    fn test_log_entry_with_raw() {
        let entry = LogEntry::new(LogLevel::Info, "app", "msg").with_raw("raw log line");
        assert_eq!(entry.raw, "raw log line");
    }

    #[test]
    fn test_log_entry_serialization() {
        let entry =
            LogEntry::new(LogLevel::Error, "app", "test message").with_field("key", "value");

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: LogEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.level, entry.level);
        assert_eq!(deserialized.source, entry.source);
        assert_eq!(deserialized.message, entry.message);
    }

    #[test]
    fn test_log_format_variants() {
        let formats = [
            LogFormat::Plain,
            LogFormat::Json,
            LogFormat::CommonLog,
            LogFormat::Syslog,
            LogFormat::Custom,
        ];
        for fmt in formats {
            let _ = format!("{:?}", fmt);
            let cloned = fmt.clone();
            assert_eq!(fmt, cloned);
        }
    }

    #[test]
    fn test_log_parser_default() {
        let parser = LogParser::default();
        assert_eq!(parser.format, LogFormat::Plain);
    }

    #[test]
    fn test_log_parser_add_pattern() {
        let mut parser = LogParser::new(LogFormat::Custom);
        parser.add_pattern("level", r"\[(\w+)\]");
        parser.add_pattern("message", r": (.+)$");

        let line = "[ERROR]: Something went wrong";
        let entry = parser.parse(line);
        assert!(entry.is_some());
    }

    #[test]
    fn test_log_parser_common_format() {
        let parser = LogParser::new(LogFormat::CommonLog);
        let line = "127.0.0.1 - frank [10/Oct/2000:13:55:36 -0700] \"GET /apache_pb.gif HTTP/1.0\" 200 2326";

        let entry = parser.parse(line);
        assert!(entry.is_some());
        let e = entry.unwrap();
        assert_eq!(e.source, "httpd");
    }

    #[test]
    fn test_log_parser_syslog_format() {
        let parser = LogParser::new(LogFormat::Syslog);
        let line = "Oct 11 22:14:15 mymachine app[1234]: Test message";

        let entry = parser.parse(line);
        assert!(entry.is_some());
    }

    #[test]
    fn test_log_parser_json_with_alternatives() {
        let parser = LogParser::new(LogFormat::Json);

        // Test with 'msg' instead of 'message'
        let line1 = r#"{"severity":"warn","msg":"Warning message"}"#;
        let entry1 = parser.parse(line1);
        assert!(entry1.is_some());
        assert_eq!(entry1.unwrap().level, LogLevel::Warn);

        // Test with 'ts' timestamp
        let line2 = r#"{"level":"info","message":"Test","ts":1234567890}"#;
        let entry2 = parser.parse(line2);
        assert!(entry2.is_some());
    }

    #[test]
    fn test_pattern_detector_error_patterns() {
        let detector = PatternDetector::default();

        let error_entry = LogEntry::new(LogLevel::Error, "app", "Database connection failed");
        let info_entry = LogEntry::new(LogLevel::Info, "app", "Request processed");

        detector.process(&error_entry);
        detector.process(&info_entry);

        let error_patterns = detector.error_patterns();
        assert_eq!(error_patterns.len(), 1);
    }

    #[test]
    fn test_pattern_detector_clear() {
        let detector = PatternDetector::default();

        let entry = LogEntry::new(LogLevel::Info, "app", "Test message");
        detector.process(&entry);
        assert!(detector.summary().patterns_detected >= 1);

        detector.clear();
        assert_eq!(detector.top_patterns(10).len(), 0);
    }

    #[test]
    fn test_pattern_summary_clone() {
        let summary = PatternSummary {
            logs_processed: 100,
            patterns_detected: 10,
            pattern_matches: 50,
            unique_patterns: 10,
        };

        let cloned = summary.clone();
        assert_eq!(summary.logs_processed, cloned.logs_processed);
    }

    #[test]
    fn test_anomaly_with_related_logs() {
        let anomaly =
            Anomaly::new(AnomalyType::ErrorSpike, 0.9, "Test").with_related_logs(vec![1, 2, 3]);

        assert_eq!(anomaly.related_logs, vec![1, 2, 3]);
    }

    #[test]
    fn test_anomaly_with_action() {
        let anomaly = Anomaly::new(AnomalyType::FrequencyAnomaly, 0.7, "Test")
            .with_action("Investigate immediately");

        assert_eq!(
            anomaly.suggested_action,
            Some("Investigate immediately".to_string())
        );
    }

    #[test]
    fn test_anomaly_type_all_variants() {
        let types = [
            AnomalyType::ErrorSpike,
            AnomalyType::FrequencyAnomaly,
            AnomalyType::NewError,
            AnomalyType::MissingLogs,
            AnomalyType::UnusualSource,
            AnomalyType::TimingAnomaly,
        ];

        for t in types {
            let _ = format!("{:?}", t);
            let cloned = t.clone();
            assert_eq!(t, cloned);
        }
    }

    #[test]
    fn test_anomaly_detector_recent_anomalies() {
        let detector = AnomalyDetector::default();
        let anomalies = detector.recent_anomalies(10);
        assert!(anomalies.is_empty());
    }

    #[test]
    fn test_anomaly_detector_clear() {
        let detector = AnomalyDetector::default();
        detector.clear();
        assert!(detector.recent_anomalies(10).is_empty());
    }

    #[test]
    fn test_anomaly_summary_clone() {
        let summary = AnomalySummary {
            logs_analyzed: 100,
            anomalies_detected: 5,
            known_sources: 3,
        };

        let cloned = summary.clone();
        assert_eq!(summary.anomalies_detected, cloned.anomalies_detected);
    }

    #[test]
    fn test_root_cause_category_all_variants() {
        let categories = [
            RootCauseCategory::Configuration,
            RootCauseCategory::Resource,
            RootCauseCategory::Network,
            RootCauseCategory::Dependency,
            RootCauseCategory::Code,
            RootCauseCategory::Data,
            RootCauseCategory::Unknown,
        ];

        for cat in categories {
            let _ = format!("{:?}", cat);
            let cloned = cat.clone();
            assert_eq!(cat, cloned);
        }
    }

    #[test]
    fn test_root_cause_analyzer_patterns() {
        let analyzer = RootCauseAnalyzer::default();

        // Out of memory
        let entries1 = vec![LogEntry::new(LogLevel::Error, "app", "OOM killed process")];
        let causes1 = analyzer.analyze(&entries1);
        assert!(!causes1.is_empty());
        assert_eq!(causes1[0].category, RootCauseCategory::Resource);

        // Timeout
        let entries2 = vec![LogEntry::new(LogLevel::Error, "app", "Request timed out")];
        let causes2 = analyzer.analyze(&entries2);
        assert!(!causes2.is_empty());

        // Permission denied
        let entries3 = vec![LogEntry::new(
            LogLevel::Error,
            "app",
            "Permission denied accessing file",
        )];
        let causes3 = analyzer.analyze(&entries3);
        assert!(!causes3.is_empty());
        assert_eq!(causes3[0].category, RootCauseCategory::Configuration);

        // Disk full
        let entries4 = vec![LogEntry::new(
            LogLevel::Error,
            "app",
            "No space left on device",
        )];
        let causes4 = analyzer.analyze(&entries4);
        assert!(!causes4.is_empty());
        assert_eq!(causes4[0].category, RootCauseCategory::Resource);
    }

    #[test]
    fn test_root_cause_analyzer_recent() {
        let analyzer = RootCauseAnalyzer::default();
        let entries = vec![LogEntry::new(LogLevel::Error, "app", "Connection refused")];
        analyzer.analyze(&entries);

        let recent = analyzer.recent_analyses(10);
        assert!(!recent.is_empty());
    }

    #[test]
    fn test_alert_status_variants() {
        let statuses = [
            AlertStatus::Open,
            AlertStatus::Acknowledged,
            AlertStatus::Resolved,
            AlertStatus::Suppressed,
        ];

        for status in statuses {
            let _ = format!("{:?}", status);
            let cloned = status.clone();
            assert_eq!(status, cloned);
        }
    }

    #[test]
    fn test_alert_correlator_correlation_window() {
        let correlator = AlertCorrelator::new(60); // 1 minute window

        let alert1 = correlator.process("Error 1", AlertSeverity::High, "app", vec![1]);
        let alert2 = correlator.process("Error 2", AlertSeverity::High, "app", vec![2]);

        // Second alert should be correlated with first
        assert_eq!(alert2.id, alert1.id);
    }

    #[test]
    fn test_alert_correlator_default() {
        let correlator = AlertCorrelator::default();
        let summary = correlator.summary();
        assert_eq!(summary.alerts_created, 0);
    }

    #[test]
    fn test_correlator_summary_clone() {
        let summary = CorrelatorSummary {
            alerts_created: 10,
            alerts_correlated: 3,
            alerts_resolved: 2,
            open_alerts: 5,
        };

        let cloned = summary.clone();
        assert_eq!(summary.alerts_created, cloned.alerts_created);
    }

    #[test]
    fn test_log_analyzer_components() {
        let analyzer = LogAnalyzer::default();

        let _patterns = analyzer.patterns();
        let _anomalies = analyzer.anomalies();
        let _alerts = analyzer.alerts();
    }

    #[test]
    fn test_log_analyzer_analyze_root_cause() {
        let analyzer = LogAnalyzer::new(LogFormat::Plain);
        analyzer.process_line("[ERROR] Connection refused to database");

        let causes = analyzer.analyze_root_cause();
        assert!(!causes.is_empty());
    }

    #[test]
    fn test_log_analyzer_summary_clone() {
        let analyzer = LogAnalyzer::default();
        let summary = analyzer.summary();
        let cloned = summary.clone();
        assert_eq!(summary.logs_stored, cloned.logs_stored);
    }

    #[test]
    fn test_log_pattern_clone() {
        let pattern = LogPattern {
            id: "pat_1".to_string(),
            template: "Error <NUM>".to_string(),
            count: 5,
            first_seen: 1000,
            last_seen: 2000,
            examples: vec!["Error 1".to_string()],
            level: LogLevel::Error,
        };

        let cloned = pattern.clone();
        assert_eq!(pattern.id, cloned.id);
        assert_eq!(pattern.count, cloned.count);
    }

    #[test]
    fn test_anomaly_severity_clamping() {
        let anomaly1 = Anomaly::new(AnomalyType::ErrorSpike, 1.5, "Test");
        assert_eq!(anomaly1.severity, 1.0);

        let anomaly2 = Anomaly::new(AnomalyType::ErrorSpike, -0.5, "Test");
        assert_eq!(anomaly2.severity, 0.0);
    }

    #[test]
    fn test_pattern_detector_threshold() {
        let detector1 = PatternDetector::new(0.5);
        let detector2 = PatternDetector::new(1.5); // Should clamp to 1.0
        let detector3 = PatternDetector::new(-0.5); // Should clamp to 0.0

        assert_eq!(detector1.threshold, 0.5);
        assert_eq!(detector2.threshold, 1.0);
        assert_eq!(detector3.threshold, 0.0);
    }
}
