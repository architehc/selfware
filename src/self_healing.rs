//! Self-Healing System
//!
//! This module provides auto-recovery capabilities:
//! - Error pattern learning and prediction
//! - Automatic recovery actions
//! - State checkpointing and restoration
//! - Proactive health monitoring
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Self-Healing Engine                       │
//! │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐   │
//! │  │ Error         │  │ Recovery      │  │ State         │   │
//! │  │ Learner       │  │ Executor      │  │ Manager       │   │
//! │  └───────────────┘  └───────────────┘  └───────────────┘   │
//! │           │                  │                  │           │
//! │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐   │
//! │  │ Pattern       │  │ Recovery      │  │ Health        │   │
//! │  │ Detector      │  │ Strategies    │  │ Predictor     │   │
//! │  └───────────────┘  └───────────────┘  └───────────────┘   │
//! └─────────────────────────────────────────────────────────────┘
//! ```

// Feature-gated module - dead_code lint disabled at crate level

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod executor;

pub use executor::{ExecutorSummary, RecoveryExecution, RecoveryExecutor};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for self-healing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHealingConfig {
    /// Enable automatic healing
    pub enabled: bool,
    /// Maximum healing attempts per error
    pub max_healing_attempts: u32,
    /// Error pattern learning window (seconds)
    pub pattern_window_secs: u64,
    /// Minimum occurrences to detect pattern
    pub pattern_threshold: u32,
    /// Enable state checkpointing
    pub enable_checkpointing: bool,
    /// Checkpoint interval (seconds)
    pub checkpoint_interval_secs: u64,
    /// Maximum checkpoints to keep
    pub max_checkpoints: usize,
    /// Enable proactive health checks
    pub proactive_monitoring: bool,
}

impl Default for SelfHealingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_healing_attempts: 3,
            pattern_window_secs: 300, // 5 minutes
            pattern_threshold: 3,
            enable_checkpointing: true,
            checkpoint_interval_secs: 60,
            max_checkpoints: 10,
            proactive_monitoring: true,
        }
    }
}

// ============================================================================
// Error Learning
// ============================================================================

/// An error occurrence for learning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorOccurrence {
    /// Error type identifier
    pub error_type: String,
    /// Error message
    pub message: String,
    /// Context (e.g., tool name, operation)
    pub context: String,
    /// Timestamp
    pub timestamp: u64,
    /// Stack trace or location
    pub location: Option<String>,
    /// Recovery action taken
    pub recovery_action: Option<String>,
    /// Whether recovery succeeded
    pub recovery_success: bool,
}

impl ErrorOccurrence {
    pub fn new(error_type: &str, message: &str, context: &str) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            error_type: error_type.to_string(),
            message: message.to_string(),
            context: context.to_string(),
            timestamp: now,
            location: None,
            recovery_action: None,
            recovery_success: false,
        }
    }

    pub fn with_location(mut self, location: &str) -> Self {
        self.location = Some(location.to_string());
        self
    }

    pub fn with_recovery(mut self, action: &str, success: bool) -> Self {
        self.recovery_action = Some(action.to_string());
        self.recovery_success = success;
        self
    }
}

/// Detected error pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPattern {
    /// Pattern identifier
    pub id: String,
    /// Error type
    pub error_type: String,
    /// Common context
    pub context_pattern: String,
    /// Occurrences
    pub occurrences: u32,
    /// First seen
    pub first_seen: u64,
    /// Last seen
    pub last_seen: u64,
    /// Recommended recovery
    pub recommended_recovery: Option<RecoveryStrategy>,
    /// Success rate of recoveries
    pub recovery_success_rate: f32,
}

/// Error pattern learner
pub struct ErrorLearner {
    config: SelfHealingConfig,
    /// Recent errors
    errors: RwLock<VecDeque<ErrorOccurrence>>,
    /// Detected patterns
    patterns: RwLock<HashMap<String, ErrorPattern>>,
    /// Recovery history
    recovery_history: RwLock<HashMap<String, Vec<RecoveryResult>>>,
    /// Statistics
    stats: LearnerStats,
}

/// Statistics for error learning
#[derive(Debug, Default)]
pub struct LearnerStats {
    pub errors_recorded: AtomicU64,
    pub patterns_detected: AtomicU64,
    pub recoveries_suggested: AtomicU64,
}

impl ErrorLearner {
    pub fn new(config: SelfHealingConfig) -> Self {
        Self {
            errors: RwLock::new(VecDeque::with_capacity(1000)),
            patterns: RwLock::new(HashMap::new()),
            recovery_history: RwLock::new(HashMap::new()),
            config,
            stats: LearnerStats::default(),
        }
    }

    /// Record an error occurrence
    pub fn record(&self, error: ErrorOccurrence) {
        self.stats.errors_recorded.fetch_add(1, Ordering::Relaxed);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Add to recent errors
        if let Ok(mut errors) = self.errors.write() {
            errors.push_back(error.clone());

            // Remove old errors outside window
            let cutoff = now.saturating_sub(self.config.pattern_window_secs);
            while errors
                .front()
                .map(|e| e.timestamp < cutoff)
                .unwrap_or(false)
            {
                errors.pop_front();
            }
        }

        // Detect patterns
        self.detect_patterns();
    }

    /// Detect error patterns
    fn detect_patterns(&self) {
        let errors = match self.errors.read() {
            Ok(e) => e.clone(),
            Err(_) => return,
        };

        // Group by error type and context
        let mut groups: HashMap<String, Vec<&ErrorOccurrence>> = HashMap::new();
        for error in errors.iter() {
            let key = format!("{}:{}", error.error_type, error.context);
            groups.entry(key).or_default().push(error);
        }

        // Create patterns from groups that exceed threshold
        if let Ok(mut patterns) = self.patterns.write() {
            for (key, group) in groups {
                if group.len() >= self.config.pattern_threshold as usize {
                    let first = group.first().unwrap();
                    let last = group.last().unwrap();

                    // Calculate recovery success rate
                    let recoveries: Vec<_> = group
                        .iter()
                        .filter(|e| e.recovery_action.is_some())
                        .collect();
                    let success_rate = if !recoveries.is_empty() {
                        recoveries.iter().filter(|e| e.recovery_success).count() as f32
                            / recoveries.len() as f32
                    } else {
                        0.0
                    };

                    // Find best recovery strategy
                    let recommended_recovery = self.find_best_recovery(&key);

                    let pattern = ErrorPattern {
                        id: key.clone(),
                        error_type: first.error_type.clone(),
                        context_pattern: first.context.clone(),
                        occurrences: group.len() as u32,
                        first_seen: first.timestamp,
                        last_seen: last.timestamp,
                        recommended_recovery,
                        recovery_success_rate: success_rate,
                    };

                    if !patterns.contains_key(&key) {
                        self.stats.patterns_detected.fetch_add(1, Ordering::Relaxed);
                    }
                    patterns.insert(key, pattern);
                }
            }
        }
    }

    /// Find best recovery strategy for an error pattern
    fn find_best_recovery(&self, pattern_id: &str) -> Option<RecoveryStrategy> {
        if let Ok(history) = self.recovery_history.read() {
            if let Some(results) = history.get(pattern_id) {
                // Find strategy with highest success rate
                let mut strategy_stats: HashMap<String, (u32, u32)> = HashMap::new(); // (success, total)

                for result in results {
                    let entry = strategy_stats.entry(result.strategy.clone()).or_default();
                    entry.1 += 1;
                    if result.success {
                        entry.0 += 1;
                    }
                }

                return strategy_stats
                    .into_iter()
                    .filter(|(_, (s, t))| *t >= 2 && *s as f32 / *t as f32 >= 0.5)
                    .max_by(|a, b| {
                        let rate_a = a.1 .0 as f32 / a.1 .1 as f32;
                        let rate_b = b.1 .0 as f32 / b.1 .1 as f32;
                        rate_a
                            .partial_cmp(&rate_b)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(strategy, _)| RecoveryStrategy::from_name(&strategy));
            }
        }
        None
    }

    /// Record recovery result
    pub fn record_recovery(&self, pattern_id: &str, strategy: &str, success: bool) {
        if let Ok(mut history) = self.recovery_history.write() {
            let results = history.entry(pattern_id.to_string()).or_default();
            results.push(RecoveryResult {
                strategy: strategy.to_string(),
                success,
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            });

            // Keep only recent results
            while results.len() > 100 {
                results.remove(0);
            }
        }
    }

    /// Get patterns
    pub fn patterns(&self) -> Vec<ErrorPattern> {
        self.patterns
            .read()
            .map(|p| p.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Get recommended recovery for error
    pub fn recommend_recovery(&self, error_type: &str, context: &str) -> Option<RecoveryStrategy> {
        let key = format!("{}:{}", error_type, context);
        self.stats
            .recoveries_suggested
            .fetch_add(1, Ordering::Relaxed);

        self.patterns
            .read()
            .ok()?
            .get(&key)?
            .recommended_recovery
            .clone()
    }

    /// Get summary
    pub fn summary(&self) -> LearnerSummary {
        LearnerSummary {
            errors_recorded: self.stats.errors_recorded.load(Ordering::Relaxed),
            patterns_detected: self.stats.patterns_detected.load(Ordering::Relaxed),
            recoveries_suggested: self.stats.recoveries_suggested.load(Ordering::Relaxed),
            active_patterns: self.patterns.read().map(|p| p.len()).unwrap_or(0),
        }
    }

    /// Clear all data
    pub fn clear(&self) {
        if let Ok(mut errors) = self.errors.write() {
            errors.clear();
        }
        if let Ok(mut patterns) = self.patterns.write() {
            patterns.clear();
        }
        if let Ok(mut history) = self.recovery_history.write() {
            history.clear();
        }
    }
}

impl Default for ErrorLearner {
    fn default() -> Self {
        Self::new(SelfHealingConfig::default())
    }
}

/// Recovery result record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryResult {
    pub strategy: String,
    pub success: bool,
    pub timestamp: u64,
}

/// Summary of error learning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnerSummary {
    pub errors_recorded: u64,
    pub patterns_detected: u64,
    pub recoveries_suggested: u64,
    pub active_patterns: usize,
}

// ============================================================================
// Recovery Strategies
// ============================================================================

/// Recovery strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryStrategy {
    /// Strategy name
    pub name: String,
    /// Description
    pub description: String,
    /// Actions to take
    pub actions: Vec<RecoveryAction>,
    /// Success probability (0.0 - 1.0)
    pub success_probability: f32,
    /// Estimated duration (ms)
    pub estimated_duration_ms: u64,
}

impl RecoveryStrategy {
    pub fn retry() -> Self {
        Self {
            name: "retry".to_string(),
            description: "Retry the failed operation".to_string(),
            actions: vec![RecoveryAction::Retry {
                delay_ms: 1000,
                max_attempts: 3,
            }],
            success_probability: 0.7,
            estimated_duration_ms: 3000,
        }
    }

    pub fn restart() -> Self {
        Self {
            name: "restart".to_string(),
            description: "Restart the affected component".to_string(),
            actions: vec![RecoveryAction::Restart {
                component: "service".to_string(),
            }],
            success_probability: 0.8,
            estimated_duration_ms: 5000,
        }
    }

    pub fn fallback() -> Self {
        Self {
            name: "fallback".to_string(),
            description: "Switch to fallback service".to_string(),
            actions: vec![RecoveryAction::Fallback {
                target: "backup".to_string(),
            }],
            success_probability: 0.9,
            estimated_duration_ms: 1000,
        }
    }

    pub fn restore() -> Self {
        Self {
            name: "restore".to_string(),
            description: "Restore from last checkpoint".to_string(),
            actions: vec![RecoveryAction::RestoreCheckpoint {
                checkpoint_id: None,
            }],
            success_probability: 0.85,
            estimated_duration_ms: 2000,
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name {
            "retry" => Self::retry(),
            "restart" => Self::restart(),
            "fallback" => Self::fallback(),
            "restore" => Self::restore(),
            _ => Self::retry(),
        }
    }
}

/// Individual recovery action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryAction {
    /// Retry operation
    Retry { delay_ms: u64, max_attempts: u32 },
    /// Restart component
    Restart { component: String },
    /// Switch to fallback
    Fallback { target: String },
    /// Restore from checkpoint
    RestoreCheckpoint { checkpoint_id: Option<String> },
    /// Clear cache
    ClearCache { scope: String },
    /// Reset state
    ResetState { scope: String },
    /// Custom action
    Custom {
        name: String,
        params: HashMap<String, String>,
    },
}

// ============================================================================
// State Management
// ============================================================================

/// State checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateCheckpoint {
    /// Checkpoint ID
    pub id: String,
    /// Description
    pub description: String,
    /// Timestamp
    pub timestamp: u64,
    /// State data
    pub state: serde_json::Value,
    /// Affected components
    pub components: Vec<String>,
    /// Size in bytes
    pub size_bytes: usize,
}

impl StateCheckpoint {
    pub fn new(description: &str, state: serde_json::Value) -> Self {
        let id = format!("ckpt_{}", uuid_v4());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let size = serde_json::to_string(&state).map(|s| s.len()).unwrap_or(0);

        Self {
            id,
            description: description.to_string(),
            timestamp: now,
            state,
            components: Vec::new(),
            size_bytes: size,
        }
    }

    pub fn with_components(mut self, components: Vec<String>) -> Self {
        self.components = components;
        self
    }
}

/// State manager for checkpointing
pub struct StateManager {
    config: SelfHealingConfig,
    /// Checkpoints
    checkpoints: RwLock<VecDeque<StateCheckpoint>>,
    /// Last checkpoint time
    last_checkpoint: RwLock<Option<Instant>>,
    /// Statistics
    stats: StateStats,
}

/// State management statistics
#[derive(Debug, Default)]
pub struct StateStats {
    pub checkpoints_created: AtomicU64,
    pub restores_performed: AtomicU64,
    pub total_bytes_saved: AtomicU64,
}

impl StateManager {
    pub fn new(config: SelfHealingConfig) -> Self {
        Self {
            checkpoints: RwLock::new(VecDeque::with_capacity(config.max_checkpoints)),
            last_checkpoint: RwLock::new(None),
            config,
            stats: StateStats::default(),
        }
    }

    /// Create a checkpoint
    pub fn checkpoint(&self, description: &str, state: serde_json::Value) -> String {
        let checkpoint = StateCheckpoint::new(description, state);
        let id = checkpoint.id.clone();

        self.stats
            .checkpoints_created
            .fetch_add(1, Ordering::Relaxed);
        self.stats
            .total_bytes_saved
            .fetch_add(checkpoint.size_bytes as u64, Ordering::Relaxed);

        if let Ok(mut checkpoints) = self.checkpoints.write() {
            checkpoints.push_back(checkpoint);

            // Limit checkpoints
            while checkpoints.len() > self.config.max_checkpoints {
                checkpoints.pop_front();
            }
        }

        if let Ok(mut last) = self.last_checkpoint.write() {
            *last = Some(Instant::now());
        }

        id
    }

    /// Check if checkpoint is needed
    pub fn needs_checkpoint(&self) -> bool {
        if !self.config.enable_checkpointing {
            return false;
        }

        if let Ok(last) = self.last_checkpoint.read() {
            if let Some(instant) = *last {
                return instant.elapsed()
                    >= Duration::from_secs(self.config.checkpoint_interval_secs);
            }
        }
        true
    }

    /// Restore from checkpoint
    pub fn restore(&self, checkpoint_id: Option<&str>) -> Option<StateCheckpoint> {
        self.stats
            .restores_performed
            .fetch_add(1, Ordering::Relaxed);

        let checkpoints = self.checkpoints.read().ok()?;

        if let Some(id) = checkpoint_id {
            checkpoints.iter().find(|c| c.id == id).cloned()
        } else {
            // Get latest checkpoint
            checkpoints.back().cloned()
        }
    }

    /// Get latest checkpoint
    pub fn latest(&self) -> Option<StateCheckpoint> {
        self.checkpoints.read().ok()?.back().cloned()
    }

    /// Get all checkpoints
    pub fn all(&self) -> Vec<StateCheckpoint> {
        self.checkpoints
            .read()
            .map(|c| c.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Clear checkpoints
    pub fn clear(&self) {
        if let Ok(mut checkpoints) = self.checkpoints.write() {
            checkpoints.clear();
        }
    }

    /// Get summary
    pub fn summary(&self) -> StateSummary {
        StateSummary {
            checkpoints_created: self.stats.checkpoints_created.load(Ordering::Relaxed),
            restores_performed: self.stats.restores_performed.load(Ordering::Relaxed),
            total_bytes_saved: self.stats.total_bytes_saved.load(Ordering::Relaxed),
            active_checkpoints: self.checkpoints.read().map(|c| c.len()).unwrap_or(0),
        }
    }
}

impl Default for StateManager {
    fn default() -> Self {
        Self::new(SelfHealingConfig::default())
    }
}

/// State management summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSummary {
    pub checkpoints_created: u64,
    pub restores_performed: u64,
    pub total_bytes_saved: u64,
    pub active_checkpoints: usize,
}

// ============================================================================
// Health Prediction
// ============================================================================

/// Health prediction for proactive healing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthPrediction {
    /// Component
    pub component: String,
    /// Predicted health status
    pub predicted_status: PredictedHealth,
    /// Confidence (0.0 - 1.0)
    pub confidence: f32,
    /// Time until predicted issue (seconds)
    pub time_until_issue_secs: Option<u64>,
    /// Recommended action
    pub recommended_action: Option<RecoveryAction>,
}

/// Predicted health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PredictedHealth {
    Healthy,
    AtRisk,
    Degrading,
    FailureImminent,
}

/// Health predictor
pub struct HealthPredictor {
    /// Health history
    history: RwLock<HashMap<String, Vec<HealthDataPoint>>>,
    /// Predictions
    predictions: RwLock<HashMap<String, HealthPrediction>>,
    /// Statistics
    stats: PredictorStats,
}

/// Health data point for trending
#[derive(Debug, Clone)]
pub struct HealthDataPoint {
    pub timestamp: u64,
    pub healthy: bool,
    pub response_time_ms: Option<u64>,
    pub error_count: u32,
}

/// Predictor statistics
#[derive(Debug, Default)]
pub struct PredictorStats {
    pub predictions_made: AtomicU64,
    pub correct_predictions: AtomicU64,
}

impl HealthPredictor {
    pub fn new() -> Self {
        Self {
            history: RwLock::new(HashMap::new()),
            predictions: RwLock::new(HashMap::new()),
            stats: PredictorStats::default(),
        }
    }

    /// Record health data point
    pub fn record(
        &self,
        component: &str,
        healthy: bool,
        response_time_ms: Option<u64>,
        error_count: u32,
    ) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Ok(mut history) = self.history.write() {
            let points = history.entry(component.to_string()).or_default();
            points.push(HealthDataPoint {
                timestamp: now,
                healthy,
                response_time_ms,
                error_count,
            });

            // Keep only last 100 points
            while points.len() > 100 {
                points.remove(0);
            }
        }

        // Update prediction
        self.update_prediction(component);
    }

    /// Update prediction for component
    fn update_prediction(&self, component: &str) {
        let history = match self.history.read() {
            Ok(h) => h.get(component).cloned().unwrap_or_default(),
            Err(_) => return,
        };

        if history.len() < 5 {
            return;
        }

        self.stats.predictions_made.fetch_add(1, Ordering::Relaxed);

        // Analyze trends
        let recent: Vec<_> = history.iter().rev().take(10).collect();
        let unhealthy_count = recent.iter().filter(|p| !p.healthy).count();
        let error_trend: u32 = recent.iter().map(|p| p.error_count).sum();
        let avg_response: f64 = recent
            .iter()
            .filter_map(|p| p.response_time_ms)
            .map(|t| t as f64)
            .sum::<f64>()
            / recent.len() as f64;

        let (status, confidence) = if unhealthy_count >= 7 {
            (PredictedHealth::FailureImminent, 0.9)
        } else if unhealthy_count >= 4 || error_trend > 10 {
            (PredictedHealth::Degrading, 0.7)
        } else if unhealthy_count >= 2 || avg_response > 3000.0 {
            (PredictedHealth::AtRisk, 0.5)
        } else {
            (PredictedHealth::Healthy, 0.8)
        };

        let recommended_action = match status {
            PredictedHealth::FailureImminent => Some(RecoveryAction::Fallback {
                target: "backup".to_string(),
            }),
            PredictedHealth::Degrading => Some(RecoveryAction::Restart {
                component: component.to_string(),
            }),
            PredictedHealth::AtRisk => Some(RecoveryAction::ClearCache {
                scope: component.to_string(),
            }),
            PredictedHealth::Healthy => None,
        };

        let prediction = HealthPrediction {
            component: component.to_string(),
            predicted_status: status,
            confidence,
            time_until_issue_secs: match status {
                PredictedHealth::FailureImminent => Some(60),
                PredictedHealth::Degrading => Some(300),
                PredictedHealth::AtRisk => Some(900),
                PredictedHealth::Healthy => None,
            },
            recommended_action,
        };

        if let Ok(mut predictions) = self.predictions.write() {
            predictions.insert(component.to_string(), prediction);
        }
    }

    /// Get prediction for component
    pub fn predict(&self, component: &str) -> Option<HealthPrediction> {
        self.predictions.read().ok()?.get(component).cloned()
    }

    /// Get all predictions
    pub fn all_predictions(&self) -> Vec<HealthPrediction> {
        self.predictions
            .read()
            .map(|p| p.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Record prediction outcome
    pub fn record_outcome(&self, _component: &str, was_correct: bool) {
        if was_correct {
            self.stats
                .correct_predictions
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Get accuracy
    pub fn accuracy(&self) -> f32 {
        let total = self.stats.predictions_made.load(Ordering::Relaxed) as f32;
        let correct = self.stats.correct_predictions.load(Ordering::Relaxed) as f32;
        if total > 0.0 {
            correct / total
        } else {
            0.0
        }
    }

    /// Clear all data
    pub fn clear(&self) {
        if let Ok(mut history) = self.history.write() {
            history.clear();
        }
        if let Ok(mut predictions) = self.predictions.write() {
            predictions.clear();
        }
    }
}

impl Default for HealthPredictor {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Error Classification
// ============================================================================

/// Classified error category used to select the best default recovery strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorClass {
    /// Network / connection errors — retry with backoff
    Network,
    /// Timeout errors — retry with longer backoff
    Timeout,
    /// Rate limiting (HTTP 429) — retry with aggressive backoff
    RateLimit,
    /// Resource exhaustion (OOM, disk full) — clear caches then retry
    ResourceExhaustion,
    /// Parse / deserialization errors — restart from checkpoint
    ParseError,
    /// Authentication / permission errors — fallback
    AuthError,
    /// Unknown — default to retry
    Unknown,
}

impl ErrorClass {
    /// Classify an error based on its type and message content.
    pub fn classify(error_type: &str, message: &str) -> Self {
        let msg = message.to_lowercase();
        let etype = error_type.to_lowercase();

        if msg.contains("rate limit") || msg.contains("429") || msg.contains("too many requests") {
            ErrorClass::RateLimit
        } else if msg.contains("timed out") || msg.contains("timeout") || etype.contains("timeout")
        {
            ErrorClass::Timeout
        } else if msg.contains("connection")
            || msg.contains("network")
            || msg.contains("dns")
            || msg.contains("refused")
            || msg.contains("reset by peer")
            || etype.contains("network")
            || etype.contains("connection")
        {
            ErrorClass::Network
        } else if msg.contains("out of memory")
            || msg.contains("oom")
            || msg.contains("disk full")
            || msg.contains("no space")
            || msg.contains("resource exhausted")
        {
            ErrorClass::ResourceExhaustion
        } else if msg.contains("parse")
            || msg.contains("invalid json")
            || msg.contains("unexpected token")
            || msg.contains("deserialize")
            || msg.contains("malformed")
        {
            ErrorClass::ParseError
        } else if msg.contains("unauthorized")
            || msg.contains("forbidden")
            || msg.contains("401")
            || msg.contains("403")
            || msg.contains("invalid api key")
        {
            ErrorClass::AuthError
        } else {
            ErrorClass::Unknown
        }
    }

    /// Return the default recovery strategy for this error class.
    pub fn default_strategy(self) -> RecoveryStrategy {
        match self {
            ErrorClass::Network => RecoveryStrategy {
                name: "network_retry".to_string(),
                description: "Retry after network error with backoff".to_string(),
                actions: vec![RecoveryAction::Retry {
                    delay_ms: 1000,
                    max_attempts: 3,
                }],
                success_probability: 0.7,
                estimated_duration_ms: 7000,
            },
            ErrorClass::Timeout => RecoveryStrategy {
                name: "timeout_retry".to_string(),
                description: "Retry after timeout with longer backoff".to_string(),
                actions: vec![RecoveryAction::Retry {
                    delay_ms: 2000,
                    max_attempts: 3,
                }],
                success_probability: 0.6,
                estimated_duration_ms: 14000,
            },
            ErrorClass::RateLimit => RecoveryStrategy {
                name: "rate_limit_backoff".to_string(),
                description: "Back off aggressively for rate limiting".to_string(),
                actions: vec![RecoveryAction::Retry {
                    delay_ms: 5000,
                    max_attempts: 5,
                }],
                success_probability: 0.85,
                estimated_duration_ms: 60000,
            },
            ErrorClass::ResourceExhaustion => RecoveryStrategy {
                name: "resource_recovery".to_string(),
                description: "Clear caches then retry".to_string(),
                actions: vec![
                    RecoveryAction::ClearCache {
                        scope: "all".to_string(),
                    },
                    RecoveryAction::Retry {
                        delay_ms: 2000,
                        max_attempts: 2,
                    },
                ],
                success_probability: 0.5,
                estimated_duration_ms: 6000,
            },
            ErrorClass::ParseError => RecoveryStrategy {
                name: "parse_restart".to_string(),
                description: "Restore from checkpoint after parse error".to_string(),
                actions: vec![RecoveryAction::RestoreCheckpoint {
                    checkpoint_id: None,
                }],
                success_probability: 0.8,
                estimated_duration_ms: 2000,
            },
            ErrorClass::AuthError => RecoveryStrategy {
                name: "auth_fallback".to_string(),
                description: "Switch to fallback on auth error".to_string(),
                actions: vec![RecoveryAction::Fallback {
                    target: "backup".to_string(),
                }],
                success_probability: 0.3,
                estimated_duration_ms: 1000,
            },
            ErrorClass::Unknown => RecoveryStrategy::retry(),
        }
    }

    /// Return the escalation strategy to try if the primary strategy fails.
    pub fn escalation_strategy(self) -> Option<RecoveryStrategy> {
        match self {
            // Network/timeout/rate-limit: escalate to restart from checkpoint
            ErrorClass::Network | ErrorClass::Timeout | ErrorClass::RateLimit => {
                Some(RecoveryStrategy {
                    name: "escalate_restart".to_string(),
                    description: "Restart from checkpoint after retry exhaustion".to_string(),
                    actions: vec![RecoveryAction::RestoreCheckpoint {
                        checkpoint_id: None,
                    }],
                    success_probability: 0.7,
                    estimated_duration_ms: 2000,
                })
            }
            // Resource exhaustion: escalate to full state reset
            ErrorClass::ResourceExhaustion => Some(RecoveryStrategy {
                name: "escalate_reset".to_string(),
                description: "Full state reset after resource recovery fails".to_string(),
                actions: vec![RecoveryAction::ResetState {
                    scope: "all".to_string(),
                }],
                success_probability: 0.6,
                estimated_duration_ms: 1000,
            }),
            // Parse errors: escalate to context compression
            ErrorClass::ParseError => Some(RecoveryStrategy {
                name: "escalate_compress".to_string(),
                description: "Compress context after parse restart fails".to_string(),
                actions: vec![RecoveryAction::Custom {
                    name: "compress_context".to_string(),
                    params: HashMap::new(),
                }],
                success_probability: 0.5,
                estimated_duration_ms: 3000,
            }),
            // Auth errors and unknown: no further escalation
            ErrorClass::AuthError | ErrorClass::Unknown => None,
        }
    }
}

// ============================================================================
// Self-Healing Engine
// ============================================================================

/// Self-healing engine — coordinates error learning, recovery execution,
/// state checkpointing, and health prediction with an escalation chain.
pub struct SelfHealingEngine {
    config: SelfHealingConfig,
    /// Error learner
    learner: ErrorLearner,
    /// State manager
    state: StateManager,
    /// Health predictor
    predictor: HealthPredictor,
    /// Recovery executor
    executor: RecoveryExecutor,
}

impl SelfHealingEngine {
    pub fn new(config: SelfHealingConfig) -> Self {
        Self {
            learner: ErrorLearner::new(config.clone()),
            state: StateManager::new(config.clone()),
            predictor: HealthPredictor::new(),
            executor: RecoveryExecutor::new(config.clone()),
            config,
        }
    }

    /// Handle an error with classification, learned strategy selection,
    /// and automatic escalation if the primary strategy fails.
    pub fn handle_error(&self, error: ErrorOccurrence) -> Option<RecoveryExecution> {
        if !self.config.enabled {
            return None;
        }

        // Record for learning
        self.learner.record(error.clone());

        let pattern_key = format!("{}:{}", error.error_type, error.context);

        // Classify the error
        let error_class = ErrorClass::classify(&error.error_type, &error.message);

        // Pick recovery strategy: learned recommendation > class-based default
        let strategy = self
            .learner
            .recommend_recovery(&error.error_type, &error.context)
            .unwrap_or_else(|| error_class.default_strategy());

        // Execute recovery with pattern tracking for exponential backoff
        let execution = self
            .executor
            .execute_for_pattern(&strategy, &self.state, &pattern_key);

        if execution.success {
            // Record successful outcome for learning
            self.learner
                .record_recovery(&pattern_key, &strategy.name, true);

            // Record healthy recovery in predictor
            self.predictor.record("self_healing", true, None, 0);

            return Some(execution);
        }

        // Primary failed — record and try escalation
        self.learner
            .record_recovery(&pattern_key, &strategy.name, false);

        // Escalate: try the next strategy in the chain
        if let Some(escalation) = error_class.escalation_strategy() {
            let escalation_key = format!("{}_escalated", pattern_key);
            let escalated_execution =
                self.executor
                    .execute_for_pattern(&escalation, &self.state, &escalation_key);

            self.learner.record_recovery(
                &pattern_key,
                &escalation.name,
                escalated_execution.success,
            );

            if !escalated_execution.success {
                self.predictor.record("self_healing", false, None, 1);
            }

            return Some(escalated_execution);
        }

        // No escalation available
        self.predictor.record("self_healing", false, None, 1);
        Some(execution)
    }

    /// Checkpoint current state
    pub fn checkpoint(&self, description: &str, state: serde_json::Value) -> String {
        self.state.checkpoint(description, state)
    }

    /// Restore from checkpoint
    pub fn restore(&self, checkpoint_id: Option<&str>) -> Option<serde_json::Value> {
        self.state.restore(checkpoint_id).map(|c| c.state)
    }

    /// Record health data
    pub fn record_health(&self, component: &str, healthy: bool, response_time_ms: Option<u64>) {
        self.predictor.record(
            component,
            healthy,
            response_time_ms,
            if healthy { 0 } else { 1 },
        );
    }

    /// Get health predictions
    pub fn predict_health(&self) -> Vec<HealthPrediction> {
        self.predictor.all_predictions()
    }

    /// Reset retry state for a pattern after a successful operation,
    /// so the next failure starts with fresh backoff.
    pub fn reset_retry(&self, error_type: &str, context: &str) {
        let pattern_key = format!("{}:{}", error_type, context);
        self.executor.reset_retry_state(&pattern_key);
    }

    /// Get components
    pub fn learner(&self) -> &ErrorLearner {
        &self.learner
    }

    pub fn state_manager(&self) -> &StateManager {
        &self.state
    }

    pub fn predictor(&self) -> &HealthPredictor {
        &self.predictor
    }

    pub fn executor(&self) -> &RecoveryExecutor {
        &self.executor
    }

    /// Get comprehensive summary
    pub fn summary(&self) -> SelfHealingSummary {
        SelfHealingSummary {
            learner: self.learner.summary(),
            state: self.state.summary(),
            executor: self.executor.summary(),
            predictor_accuracy: self.predictor.accuracy(),
            active_predictions: self.predictor.all_predictions().len(),
        }
    }
}

impl Default for SelfHealingEngine {
    fn default() -> Self {
        Self::new(SelfHealingConfig::default())
    }
}

/// Self-healing summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHealingSummary {
    pub learner: LearnerSummary,
    pub state: StateSummary,
    pub executor: ExecutorSummary,
    pub predictor_accuracy: f32,
    pub active_predictions: usize,
}

// ============================================================================
// Helper Functions
// ============================================================================

fn uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = SelfHealingConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_healing_attempts, 3);
    }

    #[test]
    fn test_error_occurrence_new() {
        let error = ErrorOccurrence::new("network", "connection failed", "api_call");
        assert_eq!(error.error_type, "network");
        assert!(!error.recovery_success);
    }

    #[test]
    fn test_error_occurrence_with_location() {
        let error = ErrorOccurrence::new("network", "err", "ctx").with_location("main.rs:42");
        assert_eq!(error.location, Some("main.rs:42".to_string()));
    }

    #[test]
    fn test_error_occurrence_with_recovery() {
        let error = ErrorOccurrence::new("network", "err", "ctx").with_recovery("retry", true);
        assert_eq!(error.recovery_action, Some("retry".to_string()));
        assert!(error.recovery_success);
    }

    #[test]
    fn test_error_learner_record() {
        let learner = ErrorLearner::default();
        learner.record(ErrorOccurrence::new("network", "err", "ctx"));

        let summary = learner.summary();
        assert_eq!(summary.errors_recorded, 1);
    }

    #[test]
    fn test_recovery_strategy_retry() {
        let strategy = RecoveryStrategy::retry();
        assert_eq!(strategy.name, "retry");
        assert!(!strategy.actions.is_empty());
    }

    #[test]
    fn test_recovery_strategy_from_name() {
        let strategy = RecoveryStrategy::from_name("fallback");
        assert_eq!(strategy.name, "fallback");
    }

    #[test]
    fn test_state_checkpoint_new() {
        let checkpoint = StateCheckpoint::new("test", serde_json::json!({"key": "value"}));
        assert!(checkpoint.id.starts_with("ckpt_"));
    }

    #[test]
    fn test_state_manager_checkpoint() {
        let manager = StateManager::default();
        let id = manager.checkpoint("test", serde_json::json!({}));
        assert!(id.starts_with("ckpt_"));

        let summary = manager.summary();
        assert_eq!(summary.checkpoints_created, 1);
    }

    #[test]
    fn test_state_manager_restore() {
        let manager = StateManager::default();
        manager.checkpoint("test", serde_json::json!({"data": 42}));

        let restored = manager.restore(None);
        assert!(restored.is_some());
    }

    #[test]
    fn test_health_predictor_record() {
        let predictor = HealthPredictor::default();
        predictor.record("api", true, Some(100), 0);
        // Should not panic
    }

    #[test]
    fn test_predicted_health_enum() {
        assert_eq!(PredictedHealth::Healthy, PredictedHealth::Healthy);
        assert_ne!(PredictedHealth::Healthy, PredictedHealth::Degrading);
    }

    #[test]
    fn test_recovery_executor_execute() {
        let executor = RecoveryExecutor::default();
        // Use zero-delay retry so the test is fast
        let strategy = RecoveryStrategy {
            name: "retry".to_string(),
            description: "test retry".to_string(),
            actions: vec![RecoveryAction::Retry {
                delay_ms: 0,
                max_attempts: 3,
            }],
            success_probability: 0.7,
            estimated_duration_ms: 0,
        };

        let result = executor.execute(&strategy);
        assert_eq!(result.strategy, "retry");
        assert!(result.completed_at.is_some());
        assert!(result.success);
    }

    #[test]
    fn test_recovery_executor_restore_requires_state_manager() {
        let executor = RecoveryExecutor::default();
        let strategy = RecoveryStrategy::restore();

        let result = executor.execute(&strategy);
        assert_eq!(result.strategy, "restore");
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_recovery_executor_restore_with_state_manager() {
        let config = SelfHealingConfig::default();
        let executor = RecoveryExecutor::new(config.clone());
        let state = StateManager::new(config);
        state.checkpoint("before_restore", serde_json::json!({"ok": true}));

        let result = executor.execute_with_state(&RecoveryStrategy::restore(), &state);
        assert!(result.success);
    }

    #[test]
    fn test_recovery_executor_clear_cache_clears_checkpoints() {
        let config = SelfHealingConfig::default();
        let executor = RecoveryExecutor::new(config.clone());
        let state = StateManager::new(config);
        state.checkpoint("to_clear", serde_json::json!({"x": 1}));

        let strategy = RecoveryStrategy {
            name: "clear_cache".to_string(),
            description: "clear".to_string(),
            actions: vec![RecoveryAction::ClearCache {
                scope: "all".to_string(),
            }],
            success_probability: 1.0,
            estimated_duration_ms: 1,
        };

        let result = executor.execute_with_state(&strategy, &state);
        assert!(result.success);
        assert!(state.restore(None).is_none());
    }

    #[test]
    fn test_recovery_executor_summary() {
        let executor = RecoveryExecutor::default();
        // Use zero-delay retry
        let strategy = RecoveryStrategy {
            name: "retry".to_string(),
            description: "test".to_string(),
            actions: vec![RecoveryAction::Retry {
                delay_ms: 0,
                max_attempts: 3,
            }],
            success_probability: 0.7,
            estimated_duration_ms: 0,
        };
        executor.execute(&strategy);

        let summary = executor.summary();
        assert_eq!(summary.executions, 1);
    }

    #[test]
    fn test_self_healing_engine_new() {
        let engine = SelfHealingEngine::default();
        let summary = engine.summary();
        assert_eq!(summary.learner.errors_recorded, 0);
    }

    #[test]
    fn test_self_healing_engine_checkpoint() {
        let engine = SelfHealingEngine::default();
        let id = engine.checkpoint("test", serde_json::json!({}));
        assert!(id.starts_with("ckpt_"));
    }

    #[test]
    fn test_self_healing_engine_restore() {
        let engine = SelfHealingEngine::default();
        engine.checkpoint("test", serde_json::json!({"value": 123}));

        let state = engine.restore(None);
        assert!(state.is_some());
    }

    #[test]
    fn test_self_healing_engine_record_health() {
        let engine = SelfHealingEngine::default();
        engine.record_health("api", true, Some(100));
        // Should not panic
    }

    #[test]
    fn test_recovery_action_variants() {
        let retry = RecoveryAction::Retry {
            delay_ms: 1000,
            max_attempts: 3,
        };
        let restart = RecoveryAction::Restart {
            component: "svc".to_string(),
        };
        let fallback = RecoveryAction::Fallback {
            target: "backup".to_string(),
        };

        // Just test that they can be created
        match retry {
            RecoveryAction::Retry { delay_ms, .. } => assert_eq!(delay_ms, 1000),
            _ => panic!("wrong variant"),
        }
        match restart {
            RecoveryAction::Restart { component } => assert_eq!(component, "svc"),
            _ => panic!("wrong variant"),
        }
        match fallback {
            RecoveryAction::Fallback { target } => assert_eq!(target, "backup"),
            _ => panic!("wrong variant"),
        }
    }

    // Additional comprehensive tests

    #[test]
    fn test_config_serialization() {
        let config = SelfHealingConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: SelfHealingConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.enabled, deserialized.enabled);
        assert_eq!(
            config.max_healing_attempts,
            deserialized.max_healing_attempts
        );
    }

    #[test]
    fn test_config_clone() {
        let config = SelfHealingConfig {
            enabled: false,
            max_healing_attempts: 5,
            ..Default::default()
        };
        let cloned = config.clone();
        assert_eq!(config.enabled, cloned.enabled);
        assert_eq!(config.max_healing_attempts, cloned.max_healing_attempts);
    }

    #[test]
    fn test_error_occurrence_serialization() {
        let error = ErrorOccurrence::new("test", "message", "context")
            .with_location("file.rs:10")
            .with_recovery("retry", true);

        let json = serde_json::to_string(&error).unwrap();
        let deserialized: ErrorOccurrence = serde_json::from_str(&json).unwrap();

        assert_eq!(error.error_type, deserialized.error_type);
        assert_eq!(error.location, deserialized.location);
    }

    #[test]
    fn test_error_occurrence_clone() {
        let error = ErrorOccurrence::new("clone_test", "msg", "ctx");
        let cloned = error.clone();
        assert_eq!(error.error_type, cloned.error_type);
    }

    #[test]
    fn test_error_learner_patterns() {
        let learner = ErrorLearner::default();

        // Record multiple similar errors
        for _ in 0..5 {
            learner.record(ErrorOccurrence::new("timeout", "request timed out", "api"));
        }

        let patterns = learner.patterns();
        assert!(!patterns.is_empty());
    }

    #[test]
    fn test_error_learner_recommend_recovery() {
        let learner = ErrorLearner::default();

        // Record errors first
        for _ in 0..5 {
            learner.record(ErrorOccurrence::new("connection", "failed", "network"));
        }

        let strategy = learner.recommend_recovery("connection", "network");
        // May or may not have a recommendation depending on pattern threshold
        if let Some(s) = strategy {
            assert!(!s.name.is_empty());
        }
    }

    #[test]
    fn test_recovery_strategy_restore() {
        let strategy = RecoveryStrategy::restore();
        assert_eq!(strategy.name, "restore");
    }

    #[test]
    fn test_recovery_strategy_clone() {
        let strategy = RecoveryStrategy::retry();
        let cloned = strategy.clone();
        assert_eq!(strategy.name, cloned.name);
    }

    #[test]
    fn test_recovery_action_serialization() {
        let actions = vec![
            RecoveryAction::Retry {
                delay_ms: 1000,
                max_attempts: 3,
            },
            RecoveryAction::Restart {
                component: "api".to_string(),
            },
            RecoveryAction::Fallback {
                target: "backup".to_string(),
            },
            RecoveryAction::RestoreCheckpoint {
                checkpoint_id: None,
            },
            RecoveryAction::ClearCache {
                scope: "all".to_string(),
            },
        ];

        for action in actions {
            let json = serde_json::to_string(&action).unwrap();
            let _: RecoveryAction = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_state_checkpoint_with_components() {
        let checkpoint = StateCheckpoint::new("test", serde_json::json!({"data": 1}))
            .with_components(vec!["comp1".to_string(), "comp2".to_string()]);

        assert_eq!(checkpoint.components.len(), 2);
    }

    #[test]
    fn test_state_checkpoint_clone() {
        let checkpoint = StateCheckpoint::new("test", serde_json::json!({}));
        let cloned = checkpoint.clone();
        assert_eq!(checkpoint.id, cloned.id);
    }

    #[test]
    fn test_state_manager_restore_by_id() {
        let manager = StateManager::default();

        let id1 = manager.checkpoint("first", serde_json::json!({"v": 1}));
        let _id2 = manager.checkpoint("second", serde_json::json!({"v": 2}));

        let restored = manager.restore(Some(&id1));
        assert!(restored.is_some());
        assert_eq!(restored.unwrap().description, "first");
    }

    #[test]
    fn test_state_manager_clear() {
        let manager = StateManager::default();

        manager.checkpoint("test", serde_json::json!({}));
        manager.clear();

        assert!(manager.restore(None).is_none());
    }

    #[test]
    fn test_health_predictor_predict() {
        let predictor = HealthPredictor::default();

        // Record healthy data points
        for _ in 0..10 {
            predictor.record("service", true, Some(50), 0);
        }

        let prediction = predictor.predict("service");
        // Prediction might be None if not enough data
        if let Some(pred) = prediction {
            assert!(!pred.component.is_empty());
        }
    }

    #[test]
    fn test_predicted_health_all_variants() {
        let variants = [
            PredictedHealth::Healthy,
            PredictedHealth::Degrading,
            PredictedHealth::AtRisk,
            PredictedHealth::FailureImminent,
        ];

        for variant in variants {
            let _ = format!("{:?}", variant);
            let cloned = variant;
            assert_eq!(variant, cloned);
        }
    }

    #[test]
    fn test_recovery_executor_new() {
        let config = SelfHealingConfig::default();
        let executor = RecoveryExecutor::new(config);
        let summary = executor.summary();
        assert_eq!(summary.executions, 0);
    }

    #[test]
    fn test_self_healing_engine_handle_error() {
        let engine = SelfHealingEngine::default();

        // Handle an error — uses zero-delay default retry
        let error = ErrorOccurrence::new("test", "msg", "ctx");
        let result = engine.handle_error(error);

        // Should return a recovery execution
        assert!(result.is_some());
    }

    #[test]
    fn test_learner_summary_clone() {
        let summary = LearnerSummary {
            errors_recorded: 10,
            patterns_detected: 5,
            recoveries_suggested: 3,
            active_patterns: 2,
        };
        let cloned = summary.clone();
        assert_eq!(summary.errors_recorded, cloned.errors_recorded);
    }

    #[test]
    fn test_state_summary_clone() {
        let summary = StateSummary {
            checkpoints_created: 5,
            restores_performed: 2,
            total_bytes_saved: 1000,
            active_checkpoints: 3,
        };
        let cloned = summary.clone();
        assert_eq!(summary.checkpoints_created, cloned.checkpoints_created);
    }

    #[test]
    fn test_executor_summary_clone() {
        let summary = ExecutorSummary {
            executions: 10,
            successes: 8,
            failures: 2,
            success_rate: 0.8,
        };
        let cloned = summary.clone();
        assert_eq!(summary.executions, cloned.executions);
    }

    #[test]
    fn test_self_healing_summary_clone() {
        let engine = SelfHealingEngine::default();
        let summary = engine.summary();
        let cloned = summary.clone();
        assert_eq!(
            summary.learner.errors_recorded,
            cloned.learner.errors_recorded
        );
    }

    #[test]
    fn test_error_learner_clear() {
        let learner = ErrorLearner::default();
        learner.record(ErrorOccurrence::new("test", "msg", "ctx"));

        learner.clear();

        let patterns = learner.patterns();
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_recovery_strategy_all_types() {
        let strategies = vec![
            RecoveryStrategy::retry(),
            RecoveryStrategy::restart(),
            RecoveryStrategy::fallback(),
            RecoveryStrategy::restore(),
            RecoveryStrategy::from_name("custom"),
        ];

        for strategy in strategies {
            assert!(!strategy.name.is_empty());
            assert!(strategy.success_probability >= 0.0);
        }
    }

    #[test]
    fn test_health_predictor_clear() {
        let predictor = HealthPredictor::default();
        predictor.record("test", true, Some(100), 0);
        predictor.clear();
        // After clear, prediction should be None
        assert!(predictor.predict("test").is_none());
    }

    // ================================================================
    // Error classification tests
    // ================================================================

    #[test]
    fn test_error_class_classify_network() {
        assert_eq!(
            ErrorClass::classify("connection", "connection refused"),
            ErrorClass::Network
        );
        assert_eq!(
            ErrorClass::classify("network", "dns resolution failed"),
            ErrorClass::Network
        );
        assert_eq!(
            ErrorClass::classify("io", "connection reset by peer"),
            ErrorClass::Network
        );
    }

    #[test]
    fn test_error_class_classify_timeout() {
        assert_eq!(
            ErrorClass::classify("timeout", "request timed out"),
            ErrorClass::Timeout
        );
        assert_eq!(
            ErrorClass::classify("api", "operation timeout after 30s"),
            ErrorClass::Timeout
        );
    }

    #[test]
    fn test_error_class_classify_rate_limit() {
        assert_eq!(
            ErrorClass::classify("api", "rate limit exceeded"),
            ErrorClass::RateLimit
        );
        assert_eq!(
            ErrorClass::classify("http", "429 Too Many Requests"),
            ErrorClass::RateLimit
        );
    }

    #[test]
    fn test_error_class_classify_resource() {
        assert_eq!(
            ErrorClass::classify("system", "out of memory"),
            ErrorClass::ResourceExhaustion
        );
        assert_eq!(
            ErrorClass::classify("io", "no space left on device"),
            ErrorClass::ResourceExhaustion
        );
    }

    #[test]
    fn test_error_class_classify_parse() {
        assert_eq!(
            ErrorClass::classify("json", "invalid json: unexpected token"),
            ErrorClass::ParseError
        );
        assert_eq!(
            ErrorClass::classify("api", "failed to deserialize response"),
            ErrorClass::ParseError
        );
    }

    #[test]
    fn test_error_class_classify_auth() {
        assert_eq!(
            ErrorClass::classify("api", "401 Unauthorized"),
            ErrorClass::AuthError
        );
        assert_eq!(
            ErrorClass::classify("auth", "invalid api key"),
            ErrorClass::AuthError
        );
    }

    #[test]
    fn test_error_class_classify_unknown() {
        assert_eq!(
            ErrorClass::classify("misc", "something weird happened"),
            ErrorClass::Unknown
        );
    }

    #[test]
    fn test_error_class_default_strategies() {
        // Each error class should produce a named strategy
        let classes = [
            ErrorClass::Network,
            ErrorClass::Timeout,
            ErrorClass::RateLimit,
            ErrorClass::ResourceExhaustion,
            ErrorClass::ParseError,
            ErrorClass::AuthError,
            ErrorClass::Unknown,
        ];

        for class in classes {
            let strategy = class.default_strategy();
            assert!(!strategy.name.is_empty());
            assert!(!strategy.actions.is_empty());
        }
    }

    #[test]
    fn test_error_class_escalation_chain() {
        // Network/timeout/rate-limit should escalate to checkpoint restore
        assert!(ErrorClass::Network.escalation_strategy().is_some());
        assert!(ErrorClass::Timeout.escalation_strategy().is_some());
        assert!(ErrorClass::RateLimit.escalation_strategy().is_some());

        // Resource exhaustion escalates to full reset
        assert!(ErrorClass::ResourceExhaustion
            .escalation_strategy()
            .is_some());

        // Parse errors escalate to context compression
        assert!(ErrorClass::ParseError.escalation_strategy().is_some());

        // Auth and unknown have no further escalation
        assert!(ErrorClass::AuthError.escalation_strategy().is_none());
        assert!(ErrorClass::Unknown.escalation_strategy().is_none());
    }

    #[test]
    fn test_error_class_serialization() {
        let class = ErrorClass::RateLimit;
        let json = serde_json::to_string(&class).unwrap();
        let deserialized: ErrorClass = serde_json::from_str(&json).unwrap();
        assert_eq!(class, deserialized);
    }

    // ================================================================
    // Retry with exponential backoff tests
    // ================================================================

    #[test]
    fn test_retry_with_zero_delay() {
        let config = SelfHealingConfig::default();
        let executor = RecoveryExecutor::new(config.clone());
        let state = StateManager::new(config);

        let strategy = RecoveryStrategy {
            name: "fast_retry".to_string(),
            description: "test".to_string(),
            actions: vec![RecoveryAction::Retry {
                delay_ms: 0,
                max_attempts: 3,
            }],
            success_probability: 1.0,
            estimated_duration_ms: 0,
        };

        // First call succeeds
        let result = executor.execute_for_pattern(&strategy, &state, "test_pattern");
        assert!(result.success);
        assert_eq!(executor.retry_attempt_count("test_pattern"), 1);

        // Second call succeeds (attempt 2)
        let result = executor.execute_for_pattern(&strategy, &state, "test_pattern");
        assert!(result.success);
        assert_eq!(executor.retry_attempt_count("test_pattern"), 2);

        // Third call succeeds (attempt 3)
        let result = executor.execute_for_pattern(&strategy, &state, "test_pattern");
        assert!(result.success);
        assert_eq!(executor.retry_attempt_count("test_pattern"), 3);

        // Fourth call exhausts max_attempts
        let result = executor.execute_for_pattern(&strategy, &state, "test_pattern");
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Max retry attempts"));
    }

    #[test]
    fn test_retry_state_reset() {
        let executor = RecoveryExecutor::default();
        let strategy = RecoveryStrategy {
            name: "retry".to_string(),
            description: "test".to_string(),
            actions: vec![RecoveryAction::Retry {
                delay_ms: 0,
                max_attempts: 2,
            }],
            success_probability: 1.0,
            estimated_duration_ms: 0,
        };

        executor.execute_for_pattern(&strategy, &StateManager::default(), "reset_test");
        assert_eq!(executor.retry_attempt_count("reset_test"), 1);

        // Reset clears the count
        executor.reset_retry_state("reset_test");
        assert_eq!(executor.retry_attempt_count("reset_test"), 0);

        // Can retry again from scratch
        executor.execute_for_pattern(&strategy, &StateManager::default(), "reset_test");
        assert_eq!(executor.retry_attempt_count("reset_test"), 1);
    }

    #[test]
    fn test_retry_zero_max_attempts_fails() {
        let executor = RecoveryExecutor::default();
        let strategy = RecoveryStrategy {
            name: "bad_retry".to_string(),
            description: "test".to_string(),
            actions: vec![RecoveryAction::Retry {
                delay_ms: 0,
                max_attempts: 0,
            }],
            success_probability: 0.0,
            estimated_duration_ms: 0,
        };

        let result = executor.execute(&strategy);
        assert!(!result.success);
        assert!(result
            .error
            .unwrap()
            .contains("max_attempts must be greater than 0"));
    }

    // ================================================================
    // Escalation tests
    // ================================================================

    #[test]
    fn test_engine_handles_network_error_with_classification() {
        let engine = SelfHealingEngine::default();

        let error = ErrorOccurrence::new("network", "connection refused", "api_call");
        let result = engine.handle_error(error);

        assert!(result.is_some());
        let execution = result.unwrap();
        // Should use network_retry strategy (classified from error)
        assert!(execution.success);
    }

    #[test]
    fn test_engine_escalates_on_retry_exhaustion() {
        let config = SelfHealingConfig {
            enabled: true,
            max_healing_attempts: 3,
            ..Default::default()
        };
        let engine = SelfHealingEngine::new(config);

        // Create a checkpoint so escalation (restore) can succeed
        engine.checkpoint("safe_state", serde_json::json!({"ok": true}));

        // Exhaust retries for the same pattern by sending multiple errors
        for i in 0..5 {
            let error = ErrorOccurrence::new("timeout", "request timed out", "api");
            let result = engine.handle_error(error);
            assert!(
                result.is_some(),
                "handle_error should always return Some when enabled (iteration {})",
                i
            );
        }
    }

    #[test]
    fn test_engine_reset_retry_after_success() {
        let engine = SelfHealingEngine::default();

        // Record an error to start retry tracking
        let error = ErrorOccurrence::new("network", "connection refused", "api");
        engine.handle_error(error);

        // After a successful operation, reset the retry state
        engine.reset_retry("network", "api");

        // The next failure should start fresh
        assert_eq!(engine.executor().retry_attempt_count("network:api"), 0);
    }

    // ================================================================
    // Multi-action strategy tests
    // ================================================================

    #[test]
    fn test_resource_exhaustion_strategy_clears_then_retries() {
        let config = SelfHealingConfig::default();
        let executor = RecoveryExecutor::new(config.clone());
        let state = StateManager::new(config);
        state.checkpoint("data", serde_json::json!({"big": "object"}));

        let strategy = ErrorClass::ResourceExhaustion.default_strategy();
        let result = executor.execute_with_state(&strategy, &state);

        assert!(result.success);
        assert_eq!(result.actions_executed.len(), 2);
        assert_eq!(result.actions_executed[0], "clear_cache");
        assert_eq!(result.actions_executed[1], "retry");

        // Cache should have been cleared
        assert!(state.restore(None).is_none());
    }

    #[test]
    fn test_restart_restores_checkpoint() {
        let config = SelfHealingConfig::default();
        let executor = RecoveryExecutor::new(config.clone());
        let state = StateManager::new(config);
        state.checkpoint("before_restart", serde_json::json!({"step": 5}));

        let strategy = RecoveryStrategy::restart();
        let result = executor.execute_with_state(&strategy, &state);

        assert!(result.success);
        assert!(result.actions_executed.contains(&"restart".to_string()));
    }

    #[test]
    fn test_custom_action_compress_context() {
        let executor = RecoveryExecutor::default();
        let strategy = RecoveryStrategy {
            name: "compress".to_string(),
            description: "test".to_string(),
            actions: vec![RecoveryAction::Custom {
                name: "compress_context".to_string(),
                params: HashMap::new(),
            }],
            success_probability: 1.0,
            estimated_duration_ms: 0,
        };

        let result = executor.execute(&strategy);
        assert!(result.success);
    }

    #[test]
    fn test_custom_action_switch_parsing_mode() {
        let executor = RecoveryExecutor::default();
        let mut params = HashMap::new();
        params.insert("mode".to_string(), "xml".to_string());

        let strategy = RecoveryStrategy {
            name: "switch".to_string(),
            description: "test".to_string(),
            actions: vec![RecoveryAction::Custom {
                name: "switch_parsing_mode".to_string(),
                params,
            }],
            success_probability: 1.0,
            estimated_duration_ms: 0,
        };

        let result = executor.execute(&strategy);
        assert!(result.success);
    }

    #[test]
    fn test_uuid_v4_format() {
        let id = uuid_v4();
        // uuid v4 format: 8-4-4-4-12 hex chars with dashes
        assert_eq!(id.len(), 36);
        assert_eq!(id.chars().filter(|c| *c == '-').count(), 4);
    }
}
