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

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

mod executor;
pub mod recovery_tree;

pub use executor::{ExecutorSummary, RecoveryExecution, RecoveryExecutor};
pub use recovery_tree::{
    classify, ContextOverflowCompress, CredentialReload, DeadlockDetector, FailureKind,
    FailureSignal, LocalEndpointFallback, RecoveryContext, RecoveryDirective, RecoveryTree,
    Resolution, ResolutionOutcome,
};

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
        {
            let mut errors = self.errors.write();
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
        let errors = self.errors.read().clone();

        // Group by error type and context
        let mut groups: HashMap<String, Vec<&ErrorOccurrence>> = HashMap::new();
        for error in errors.iter() {
            let key = format!("{}:{}", error.error_type, error.context);
            groups.entry(key).or_default().push(error);
        }

        // Create patterns from groups that exceed threshold
        {
            let mut patterns = self.patterns.write();
            for (key, group) in groups {
                if group.len() >= self.config.pattern_threshold as usize {
                    let first = group.first().expect("group is non-empty");
                    let last = group.last().expect("group is non-empty");

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
        let history = self.recovery_history.read();
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
        None
    }

    /// Record recovery result
    pub fn record_recovery(&self, pattern_id: &str, strategy: &str, success: bool) {
        let mut history = self.recovery_history.write();
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

    /// Get patterns
    pub fn patterns(&self) -> Vec<ErrorPattern> {
        self.patterns.read().values().cloned().collect()
    }

    /// Get recommended recovery for error
    pub fn recommend_recovery(&self, error_type: &str, context: &str) -> Option<RecoveryStrategy> {
        let key = format!("{}:{}", error_type, context);
        self.stats
            .recoveries_suggested
            .fetch_add(1, Ordering::Relaxed);

        self.patterns.read().get(&key)?.recommended_recovery.clone()
    }

    /// Get summary
    pub fn summary(&self) -> LearnerSummary {
        LearnerSummary {
            errors_recorded: self.stats.errors_recorded.load(Ordering::Relaxed),
            patterns_detected: self.stats.patterns_detected.load(Ordering::Relaxed),
            recoveries_suggested: self.stats.recoveries_suggested.load(Ordering::Relaxed),
            active_patterns: self.patterns.read().len(),
        }
    }

    /// Clear all data
    pub fn clear(&self) {
        self.errors.write().clear();
        self.patterns.write().clear();
        self.recovery_history.write().clear();
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

        {
            let mut checkpoints = self.checkpoints.write();
            checkpoints.push_back(checkpoint);

            // Limit checkpoints
            while checkpoints.len() > self.config.max_checkpoints {
                checkpoints.pop_front();
            }
        }

        *self.last_checkpoint.write() = Some(Instant::now());

        id
    }

    /// Restore from checkpoint
    pub fn restore(&self, checkpoint_id: Option<&str>) -> Option<StateCheckpoint> {
        self.stats
            .restores_performed
            .fetch_add(1, Ordering::Relaxed);

        let checkpoints = self.checkpoints.read();

        if let Some(id) = checkpoint_id {
            checkpoints.iter().find(|c| c.id == id).cloned()
        } else {
            // Get latest checkpoint
            checkpoints.back().cloned()
        }
    }

    /// Get latest checkpoint
    pub fn latest(&self) -> Option<StateCheckpoint> {
        self.checkpoints.read().back().cloned()
    }

    /// Get all checkpoints
    pub fn all(&self) -> Vec<StateCheckpoint> {
        self.checkpoints.read().iter().cloned().collect()
    }

    /// Clear checkpoints
    pub fn clear(&self) {
        self.checkpoints.write().clear();
    }

    /// Get summary
    pub fn summary(&self) -> StateSummary {
        StateSummary {
            checkpoints_created: self.stats.checkpoints_created.load(Ordering::Relaxed),
            restores_performed: self.stats.restores_performed.load(Ordering::Relaxed),
            total_bytes_saved: self.stats.total_bytes_saved.load(Ordering::Relaxed),
            active_checkpoints: self.checkpoints.read().len(),
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

        {
            let mut history = self.history.write();
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
        let history = self
            .history
            .read()
            .get(component)
            .cloned()
            .unwrap_or_default();

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

        self.predictions
            .write()
            .insert(component.to_string(), prediction);
    }

    /// Get prediction for component
    pub fn predict(&self, component: &str) -> Option<HealthPrediction> {
        self.predictions.read().get(component).cloned()
    }

    /// Get all predictions
    pub fn all_predictions(&self) -> Vec<HealthPrediction> {
        self.predictions.read().values().cloned().collect()
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
        self.history.write().clear();
        self.predictions.write().clear();
    }
}

impl Default for HealthPredictor {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Persistence
// ============================================================================

/// Serializable snapshot of `PredictorStats` (which uses `AtomicU64`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictorStatsSnapshot {
    pub predictions_made: u64,
    pub correct_predictions: u64,
}

impl PredictorStats {
    /// Create a serializable snapshot.
    pub fn snapshot(&self) -> PredictorStatsSnapshot {
        PredictorStatsSnapshot {
            predictions_made: self.predictions_made.load(Ordering::Relaxed),
            correct_predictions: self.correct_predictions.load(Ordering::Relaxed),
        }
    }

    /// Restore from a snapshot.
    pub fn restore(&self, snap: &PredictorStatsSnapshot) {
        self.predictions_made
            .store(snap.predictions_made, Ordering::Relaxed);
        self.correct_predictions
            .store(snap.correct_predictions, Ordering::Relaxed);
    }
}

/// Serializable state of `HealthPredictor`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthPredictorState {
    pub history: HashMap<String, Vec<HealthDataPoint>>,
    pub stats: PredictorStatsSnapshot,
}

impl HealthPredictor {
    /// Save history and stats to a JSON file.
    pub fn save_history(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let history = self.history.read().clone();
        let state = HealthPredictorState {
            history,
            stats: self.stats.snapshot(),
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&state)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load history and stats from a JSON file.
    pub fn load_history(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let data = std::fs::read_to_string(path)?;
        let state: HealthPredictorState = serde_json::from_str(&data)?;
        *self.history.write() = state.history;
        self.stats.restore(&state.stats);
        Ok(())
    }
}

/// Serializable state of `ErrorLearner`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorLearnerState {
    pub patterns: HashMap<String, ErrorPattern>,
    pub recovery_history: HashMap<String, Vec<RecoveryResult>>,
}

impl ErrorLearner {
    /// Save patterns and recovery history to a JSON file.
    pub fn save_patterns(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let patterns = self.patterns.read().clone();
        let recovery_history = self.recovery_history.read().clone();
        let state = ErrorLearnerState {
            patterns,
            recovery_history,
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&state)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load patterns and recovery history from a JSON file.
    pub fn load_patterns(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let data = std::fs::read_to_string(path)?;
        let state: ErrorLearnerState = serde_json::from_str(&data)?;
        *self.patterns.write() = state.patterns;
        *self.recovery_history.write() = state.recovery_history;
        Ok(())
    }
}

impl SelfHealingEngine {
    /// Save all persistent state to a directory.
    pub fn save_state(&self, dir: &std::path::Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(dir)?;
        self.predictor
            .save_history(&dir.join("health_predictor.json"))?;
        self.learner
            .save_patterns(&dir.join("error_learner.json"))?;
        Ok(())
    }

    /// Load persistent state from a directory. Missing files are skipped gracefully.
    pub fn load_state(&self, dir: &std::path::Path) -> anyhow::Result<()> {
        let predictor_path = dir.join("health_predictor.json");
        if predictor_path.exists() {
            self.predictor.load_history(&predictor_path)?;
        }
        let learner_path = dir.join("error_learner.json");
        if learner_path.exists() {
            self.learner.load_patterns(&learner_path)?;
        }
        Ok(())
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
    /// Classify an `anyhow::Error` by attempting typed downcasts first,
    /// then falling back to the string-based [`classify`](Self::classify) method.
    ///
    /// Typed checks are more robust than string matching because they survive
    /// refactoring -- if an upstream library changes its error messages, the
    /// typed downcast still works. String matching is kept as a fallback for
    /// errors that cannot be represented by a concrete type.
    pub fn classify_anyhow(error: &anyhow::Error) -> Self {
        // --- Typed downcasts (preferred) ---

        // reqwest::Error -- covers network, timeout, and HTTP status errors
        if let Some(reqwest_err) = error.downcast_ref::<reqwest::Error>() {
            if reqwest_err.is_timeout() {
                return ErrorClass::Timeout;
            }
            if reqwest_err.is_connect() {
                return ErrorClass::Network;
            }
            if let Some(status) = reqwest_err.status() {
                return match status.as_u16() {
                    429 => ErrorClass::RateLimit,
                    401 | 403 => ErrorClass::AuthError,
                    _ => ErrorClass::Network,
                };
            }
            // Other reqwest errors (e.g., redirect, body, decode) are network-ish
            return ErrorClass::Network;
        }

        // std::io::Error -- covers OS-level failures
        if let Some(io_err) = error.downcast_ref::<std::io::Error>() {
            return match io_err.kind() {
                std::io::ErrorKind::TimedOut => ErrorClass::Timeout,
                std::io::ErrorKind::ConnectionRefused
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::NotConnected
                | std::io::ErrorKind::AddrNotAvailable
                | std::io::ErrorKind::AddrInUse => ErrorClass::Network,
                std::io::ErrorKind::PermissionDenied => ErrorClass::AuthError,
                std::io::ErrorKind::OutOfMemory => ErrorClass::ResourceExhaustion,
                _ => {
                    // Fall through to string matching for uncommon io errors
                    let msg = io_err.to_string();
                    if msg.contains("no space") || msg.contains("disk full") {
                        ErrorClass::ResourceExhaustion
                    } else {
                        ErrorClass::Unknown
                    }
                }
            };
        }

        // serde_json::Error -- JSON parse/deserialization failures
        if error.downcast_ref::<serde_json::Error>().is_some() {
            return ErrorClass::ParseError;
        }

        // --- Fallback: string-based classification ---
        let message = format!("{:#}", error);
        let error_type = error
            .chain()
            .last()
            .map(|e| {
                // Use the root cause type name when available via Debug
                let dbg = format!("{:?}", e);
                // Extract just the type prefix if it looks like "TypeName { ... }"
                dbg.split_whitespace()
                    .next()
                    .unwrap_or("unknown")
                    .to_string()
            })
            .unwrap_or_else(|| "unknown".to_string());
        Self::classify(&error_type, &message)
    }

    /// Classify an error based on its type and message content.
    ///
    /// This is the string-based fallback used when the original typed error is
    /// not available. Prefer [`classify_anyhow`](Self::classify_anyhow) when
    /// you have an `anyhow::Error`.
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
    pub async fn handle_error(&self, error: ErrorOccurrence) -> Option<RecoveryExecution> {
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
            .execute_for_pattern(&strategy, &self.state, &pattern_key)
            .await;

        if execution.success {
            // Record successful outcome for learning
            self.learner
                .record_recovery(&pattern_key, &strategy.name, true);

            // Record healthy recovery in predictor
            self.predictor.record("self_healing", true, None, 0);

            // Reset backoff state so the next failure starts with fresh retry
            // counters instead of inheriting the elevated delay from this cycle.
            self.executor.reset_retry_state(&pattern_key);

            return Some(execution);
        }

        // Primary failed — record and try escalation
        self.learner
            .record_recovery(&pattern_key, &strategy.name, false);

        // Escalate: try the next strategy in the chain
        if let Some(escalation) = error_class.escalation_strategy() {
            let escalation_key = format!("{}_escalated", pattern_key);
            let escalated_execution = self
                .executor
                .execute_for_pattern(&escalation, &self.state, &escalation_key)
                .await;

            self.learner.record_recovery(
                &pattern_key,
                &escalation.name,
                escalated_execution.success,
            );

            if escalated_execution.success {
                // Reset backoff for both the primary and escalated patterns
                // so subsequent failures start from a clean slate.
                self.executor.reset_retry_state(&pattern_key);
                self.executor.reset_retry_state(&escalation_key);
            } else {
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
#[path = "../../tests/unit/self_healing/mod_test.rs"]
mod tests;
