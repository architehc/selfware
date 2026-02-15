//! Speculative Execution Framework
//!
//! This module provides speculative execution capabilities:
//! - Predict likely next steps based on patterns
//! - Pre-fetch files before they're needed
//! - Warm caches with predicted content
//! - Parallel hypothesis testing
//! - Rollback on misprediction
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                 Speculative Executor                        │
//! │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐   │
//! │  │ Predictor     │  │ Prefetch      │  │ Cache         │   │
//! │  │ (patterns)    │  │ Manager       │  │ Warmer        │   │
//! │  └───────────────┘  └───────────────┘  └───────────────┘   │
//! │           │                  │                  │           │
//! │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐   │
//! │  │ Hypothesis    │  │ Rollback      │  │ Stats         │   │
//! │  │ Tester        │  │ Manager       │  │ Tracker       │   │
//! │  └───────────────┘  └───────────────┘  └───────────────┘   │
//! └─────────────────────────────────────────────────────────────┘
//! ```

// Feature-gated module - dead_code lint disabled at crate level

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for speculative execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeculativeConfig {
    /// Enable speculative execution
    pub enabled: bool,
    /// Maximum number of speculative operations
    pub max_speculative_ops: usize,
    /// Minimum confidence threshold for speculation (0.0 - 1.0)
    pub min_confidence: f32,
    /// Maximum depth of speculation (how many steps ahead)
    pub max_depth: usize,
    /// Enable file pre-fetching
    pub prefetch_files: bool,
    /// Maximum files to pre-fetch
    pub max_prefetch_files: usize,
    /// Enable cache warming
    pub warm_cache: bool,
    /// Enable parallel hypothesis testing
    pub hypothesis_testing: bool,
    /// Maximum parallel hypotheses
    pub max_hypotheses: usize,
}

impl Default for SpeculativeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_speculative_ops: 10,
            min_confidence: 0.7,
            max_depth: 3,
            prefetch_files: true,
            max_prefetch_files: 20,
            warm_cache: true,
            hypothesis_testing: true,
            max_hypotheses: 4,
        }
    }
}

// ============================================================================
// Prediction
// ============================================================================

/// A predicted next step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prediction {
    /// Unique identifier
    pub id: String,
    /// Predicted tool name
    pub tool_name: String,
    /// Predicted arguments
    pub arguments: serde_json::Value,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Reasoning for the prediction
    pub reasoning: String,
    /// Depth in prediction chain
    pub depth: usize,
    /// Parent prediction ID (if any)
    pub parent_id: Option<String>,
}

impl Prediction {
    /// Create a new prediction
    pub fn new(tool_name: &str, arguments: serde_json::Value, confidence: f32) -> Self {
        let id = format!("pred_{}", uuid_v4());
        Self {
            id,
            tool_name: tool_name.to_string(),
            arguments,
            confidence,
            reasoning: String::new(),
            depth: 0,
            parent_id: None,
        }
    }

    /// Add reasoning
    pub fn with_reasoning(mut self, reasoning: &str) -> Self {
        self.reasoning = reasoning.to_string();
        self
    }

    /// Set depth
    pub fn with_depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }

    /// Set parent
    pub fn with_parent(mut self, parent_id: &str) -> Self {
        self.parent_id = Some(parent_id.to_string());
        self
    }
}

/// Pattern-based predictor for next steps
pub struct Predictor {
    /// Historical action sequences
    sequences: RwLock<VecDeque<ActionSequence>>,
    /// Pattern frequency counts
    patterns: RwLock<HashMap<String, PatternStats>>,
    /// Configuration
    config: SpeculativeConfig,
    /// Statistics
    stats: PredictorStats,
}

/// A sequence of actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSequence {
    /// Actions in order
    pub actions: Vec<String>,
    /// Timestamp
    pub timestamp: u64,
    /// Context hash
    pub context_hash: u64,
}

/// Statistics for a pattern
#[derive(Debug, Clone, Default)]
pub struct PatternStats {
    /// Total occurrences
    pub count: u64,
    /// Successful predictions
    pub successes: u64,
    /// Average confidence
    pub avg_confidence: f32,
}

/// Predictor statistics
#[derive(Debug, Default)]
pub struct PredictorStats {
    pub total_predictions: AtomicU64,
    pub correct_predictions: AtomicU64,
    pub cache_hits: AtomicU64,
}

impl Predictor {
    /// Create a new predictor
    pub fn new(config: SpeculativeConfig) -> Self {
        Self {
            sequences: RwLock::new(VecDeque::with_capacity(1000)),
            patterns: RwLock::new(HashMap::new()),
            config,
            stats: PredictorStats::default(),
        }
    }

    /// Record an action for learning
    pub fn record_action(&self, action: &str, context_hash: u64) {
        if let Ok(mut sequences) = self.sequences.write() {
            // Add to current sequence or start new one
            if let Some(last) = sequences.back_mut() {
                if last.context_hash == context_hash {
                    last.actions.push(action.to_string());
                    self.update_patterns(&last.actions);
                    return;
                }
            }

            // Start new sequence
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            sequences.push_back(ActionSequence {
                actions: vec![action.to_string()],
                timestamp: now,
                context_hash,
            });

            // Limit history
            while sequences.len() > 1000 {
                sequences.pop_front();
            }
        }
    }

    /// Update patterns from an action sequence
    fn update_patterns(&self, actions: &[String]) {
        if actions.len() < 2 {
            return;
        }

        // Create pattern key from recent actions
        let pattern_key = if actions.len() >= 3 {
            format!(
                "{}->{}->",
                actions[actions.len() - 3],
                actions[actions.len() - 2]
            )
        } else {
            format!("{}->", actions[actions.len() - 2])
        };

        let next_action = &actions[actions.len() - 1];
        let full_pattern = format!("{}{}", pattern_key, next_action);

        if let Ok(mut patterns) = self.patterns.write() {
            let stats = patterns.entry(full_pattern).or_default();
            stats.count += 1;
        }
    }

    /// Predict next steps based on current action
    pub fn predict(&self, current_action: &str, _context_hash: u64) -> Vec<Prediction> {
        self.stats.total_predictions.fetch_add(1, Ordering::Relaxed);

        let mut predictions = Vec::new();

        // Try to find matching patterns
        if let Ok(patterns) = self.patterns.read() {
            let prefix = format!("{}->", current_action);

            for (pattern, stats) in patterns.iter() {
                if pattern.starts_with(&prefix) && stats.count >= 2 {
                    // Extract predicted action from pattern
                    if let Some(next_action) = pattern.strip_prefix(&prefix) {
                        let confidence = (stats.count as f32 / 10.0).min(0.95);

                        if confidence >= self.config.min_confidence {
                            let prediction =
                                Prediction::new(next_action, serde_json::json!({}), confidence)
                                    .with_reasoning(&format!("Pattern seen {} times", stats.count));

                            predictions.push(prediction);
                        }
                    }
                }
            }
        }

        // Sort by confidence
        predictions.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Limit to max speculative ops
        predictions.truncate(self.config.max_speculative_ops);

        predictions
    }

    /// Record prediction outcome
    pub fn record_outcome(&self, prediction: &Prediction, was_correct: bool) {
        if was_correct {
            self.stats
                .correct_predictions
                .fetch_add(1, Ordering::Relaxed);

            if let Ok(mut patterns) = self.patterns.write() {
                let pattern_key = format!("{}->", prediction.tool_name);
                for (pattern, stats) in patterns.iter_mut() {
                    if pattern.starts_with(&pattern_key) {
                        stats.successes += 1;
                        stats.avg_confidence = (stats.avg_confidence
                            * (stats.successes - 1) as f32
                            + prediction.confidence)
                            / stats.successes as f32;
                    }
                }
            }
        }
    }

    /// Get prediction accuracy
    pub fn accuracy(&self) -> f32 {
        let total = self.stats.total_predictions.load(Ordering::Relaxed) as f32;
        let correct = self.stats.correct_predictions.load(Ordering::Relaxed) as f32;
        if total > 0.0 {
            correct / total
        } else {
            0.0
        }
    }

    /// Clear all patterns
    pub fn clear(&self) {
        if let Ok(mut sequences) = self.sequences.write() {
            sequences.clear();
        }
        if let Ok(mut patterns) = self.patterns.write() {
            patterns.clear();
        }
    }
}

impl Default for Predictor {
    fn default() -> Self {
        Self::new(SpeculativeConfig::default())
    }
}

// ============================================================================
// Pre-fetching
// ============================================================================

/// Manages file pre-fetching
pub struct PrefetchManager {
    /// Files being prefetched
    pending: RwLock<HashSet<String>>,
    /// Prefetched file contents
    cache: RwLock<HashMap<String, PrefetchedFile>>,
    /// Configuration
    config: SpeculativeConfig,
    /// Statistics
    stats: PrefetchStats,
}

/// A pre-fetched file
#[derive(Debug, Clone)]
pub struct PrefetchedFile {
    /// File path
    pub path: String,
    /// File content
    pub content: String,
    /// Fetch timestamp
    pub fetched_at: u64,
    /// Whether it was used
    pub was_used: bool,
}

/// Pre-fetch statistics
#[derive(Debug, Default)]
pub struct PrefetchStats {
    pub total_prefetched: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub bytes_prefetched: AtomicU64,
}

impl PrefetchManager {
    /// Create a new prefetch manager
    pub fn new(config: SpeculativeConfig) -> Self {
        Self {
            pending: RwLock::new(HashSet::new()),
            cache: RwLock::new(HashMap::new()),
            config,
            stats: PrefetchStats::default(),
        }
    }

    /// Schedule a file for pre-fetching
    pub fn schedule(&self, path: &str) -> bool {
        if !self.config.prefetch_files {
            return false;
        }

        if let Ok(mut pending) = self.pending.write() {
            if pending.len() < self.config.max_prefetch_files {
                pending.insert(path.to_string());
                return true;
            }
        }
        false
    }

    /// Execute scheduled prefetches
    pub fn execute_prefetches(&self) {
        let paths: Vec<String> = self
            .pending
            .write()
            .map(|mut p| p.drain().collect())
            .unwrap_or_default();

        for path in paths {
            if let Ok(content) = std::fs::read_to_string(&path) {
                self.stats
                    .bytes_prefetched
                    .fetch_add(content.len() as u64, Ordering::Relaxed);
                self.stats.total_prefetched.fetch_add(1, Ordering::Relaxed);

                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                if let Ok(mut cache) = self.cache.write() {
                    cache.insert(
                        path.clone(),
                        PrefetchedFile {
                            path,
                            content,
                            fetched_at: now,
                            was_used: false,
                        },
                    );

                    // Evict old entries
                    while cache.len() > self.config.max_prefetch_files * 2 {
                        if let Some(oldest) = cache
                            .iter()
                            .min_by_key(|(_, v)| v.fetched_at)
                            .map(|(k, _)| k.clone())
                        {
                            cache.remove(&oldest);
                        } else {
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Try to get a prefetched file
    pub fn get(&self, path: &str) -> Option<String> {
        if let Ok(mut cache) = self.cache.write() {
            if let Some(entry) = cache.get_mut(path) {
                entry.was_used = true;
                self.stats.cache_hits.fetch_add(1, Ordering::Relaxed);
                return Some(entry.content.clone());
            }
        }
        self.stats.cache_misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Get hit rate
    pub fn hit_rate(&self) -> f32 {
        let hits = self.stats.cache_hits.load(Ordering::Relaxed) as f32;
        let misses = self.stats.cache_misses.load(Ordering::Relaxed) as f32;
        let total = hits + misses;
        if total > 0.0 {
            hits / total
        } else {
            0.0
        }
    }

    /// Get summary statistics
    pub fn summary(&self) -> PrefetchSummary {
        PrefetchSummary {
            total_prefetched: self.stats.total_prefetched.load(Ordering::Relaxed),
            cache_hits: self.stats.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.stats.cache_misses.load(Ordering::Relaxed),
            bytes_prefetched: self.stats.bytes_prefetched.load(Ordering::Relaxed),
            hit_rate: self.hit_rate(),
        }
    }

    /// Clear cache
    pub fn clear(&self) {
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
        if let Ok(mut pending) = self.pending.write() {
            pending.clear();
        }
    }
}

impl Default for PrefetchManager {
    fn default() -> Self {
        Self::new(SpeculativeConfig::default())
    }
}

/// Summary of prefetch statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefetchSummary {
    pub total_prefetched: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub bytes_prefetched: u64,
    pub hit_rate: f32,
}

// ============================================================================
// Cache Warming
// ============================================================================

/// Manages cache warming for predicted operations
pub struct CacheWarmer {
    /// Pending warm requests
    pending: RwLock<Vec<WarmRequest>>,
    /// Warmed entries
    warmed: RwLock<HashMap<String, WarmEntry>>,
    /// Configuration
    config: SpeculativeConfig,
    /// Statistics
    stats: WarmStats,
}

/// A request to warm cache
#[derive(Debug, Clone)]
pub struct WarmRequest {
    /// Cache key
    pub key: String,
    /// Value to cache
    pub value: serde_json::Value,
    /// Priority
    pub priority: u32,
}

/// A warmed cache entry
#[derive(Debug, Clone)]
pub struct WarmEntry {
    /// Cache key
    pub key: String,
    /// Cached value
    pub value: serde_json::Value,
    /// Warmed at timestamp
    pub warmed_at: u64,
    /// Whether it was used
    pub was_used: bool,
}

/// Cache warming statistics
#[derive(Debug, Default)]
pub struct WarmStats {
    pub total_warmed: AtomicU64,
    pub cache_hits: AtomicU64,
    pub wasted_warms: AtomicU64,
}

impl CacheWarmer {
    /// Create a new cache warmer
    pub fn new(config: SpeculativeConfig) -> Self {
        Self {
            pending: RwLock::new(Vec::new()),
            warmed: RwLock::new(HashMap::new()),
            config,
            stats: WarmStats::default(),
        }
    }

    /// Schedule a cache warm
    pub fn schedule(&self, key: &str, value: serde_json::Value, priority: u32) {
        if !self.config.warm_cache {
            return;
        }

        if let Ok(mut pending) = self.pending.write() {
            pending.push(WarmRequest {
                key: key.to_string(),
                value,
                priority,
            });

            // Sort by priority
            pending.sort_by(|a, b| b.priority.cmp(&a.priority));

            // Limit pending
            pending.truncate(self.config.max_speculative_ops);
        }
    }

    /// Execute pending warm requests
    pub fn execute_warms(&self) {
        let requests: Vec<WarmRequest> = self
            .pending
            .write()
            .map(|mut p| p.drain(..).collect())
            .unwrap_or_default();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Ok(mut warmed) = self.warmed.write() {
            for req in requests {
                self.stats.total_warmed.fetch_add(1, Ordering::Relaxed);
                warmed.insert(
                    req.key.clone(),
                    WarmEntry {
                        key: req.key,
                        value: req.value,
                        warmed_at: now,
                        was_used: false,
                    },
                );
            }
        }
    }

    /// Try to get a warmed entry
    pub fn get(&self, key: &str) -> Option<serde_json::Value> {
        if let Ok(mut warmed) = self.warmed.write() {
            if let Some(entry) = warmed.get_mut(key) {
                entry.was_used = true;
                self.stats.cache_hits.fetch_add(1, Ordering::Relaxed);
                return Some(entry.value.clone());
            }
        }
        None
    }

    /// Calculate efficiency (used / total)
    pub fn efficiency(&self) -> f32 {
        let total = self.stats.total_warmed.load(Ordering::Relaxed) as f32;
        let hits = self.stats.cache_hits.load(Ordering::Relaxed) as f32;
        if total > 0.0 {
            hits / total
        } else {
            0.0
        }
    }

    /// Clean up unused entries
    pub fn cleanup(&self) {
        if let Ok(mut warmed) = self.warmed.write() {
            let before = warmed.len();
            warmed.retain(|_, v| v.was_used);
            let removed = before - warmed.len();
            self.stats
                .wasted_warms
                .fetch_add(removed as u64, Ordering::Relaxed);
        }
    }

    /// Clear all
    pub fn clear(&self) {
        if let Ok(mut pending) = self.pending.write() {
            pending.clear();
        }
        if let Ok(mut warmed) = self.warmed.write() {
            warmed.clear();
        }
    }

    /// Get summary
    pub fn summary(&self) -> WarmSummary {
        WarmSummary {
            total_warmed: self.stats.total_warmed.load(Ordering::Relaxed),
            cache_hits: self.stats.cache_hits.load(Ordering::Relaxed),
            wasted_warms: self.stats.wasted_warms.load(Ordering::Relaxed),
            efficiency: self.efficiency(),
        }
    }
}

impl Default for CacheWarmer {
    fn default() -> Self {
        Self::new(SpeculativeConfig::default())
    }
}

/// Summary of cache warming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmSummary {
    pub total_warmed: u64,
    pub cache_hits: u64,
    pub wasted_warms: u64,
    pub efficiency: f32,
}

// ============================================================================
// Hypothesis Testing
// ============================================================================

/// A hypothesis to test
#[derive(Debug, Clone)]
pub struct Hypothesis {
    /// Unique identifier
    pub id: String,
    /// Description
    pub description: String,
    /// Actions to execute
    pub actions: Vec<HypothesisAction>,
    /// Confidence
    pub confidence: f32,
    /// Status
    pub status: HypothesisStatus,
    /// Result (if completed)
    pub result: Option<HypothesisResult>,
}

/// An action in a hypothesis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HypothesisAction {
    /// Tool name
    pub tool_name: String,
    /// Arguments
    pub arguments: serde_json::Value,
    /// Order in sequence
    pub order: usize,
}

/// Status of a hypothesis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HypothesisStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

/// Result of testing a hypothesis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HypothesisResult {
    /// Was it correct?
    pub correct: bool,
    /// Confidence after testing
    pub final_confidence: f32,
    /// Duration in ms
    pub duration_ms: u64,
    /// Error message if failed
    pub error: Option<String>,
}

impl Hypothesis {
    /// Create a new hypothesis
    pub fn new(description: &str, confidence: f32) -> Self {
        Self {
            id: format!("hyp_{}", uuid_v4()),
            description: description.to_string(),
            actions: Vec::new(),
            confidence,
            status: HypothesisStatus::Pending,
            result: None,
        }
    }

    /// Add an action
    pub fn add_action(&mut self, tool_name: &str, arguments: serde_json::Value) {
        let order = self.actions.len();
        self.actions.push(HypothesisAction {
            tool_name: tool_name.to_string(),
            arguments,
            order,
        });
    }

    /// Mark as succeeded
    pub fn succeed(&mut self, duration_ms: u64) {
        self.status = HypothesisStatus::Succeeded;
        self.result = Some(HypothesisResult {
            correct: true,
            final_confidence: self.confidence,
            duration_ms,
            error: None,
        });
    }

    /// Mark as failed
    pub fn fail(&mut self, duration_ms: u64, error: Option<String>) {
        self.status = HypothesisStatus::Failed;
        self.result = Some(HypothesisResult {
            correct: false,
            final_confidence: 0.0,
            duration_ms,
            error,
        });
    }

    /// Cancel
    pub fn cancel(&mut self) {
        self.status = HypothesisStatus::Cancelled;
    }
}

/// Manages parallel hypothesis testing
pub struct HypothesisTester {
    /// Active hypotheses
    hypotheses: RwLock<HashMap<String, Hypothesis>>,
    /// Configuration
    config: SpeculativeConfig,
    /// Statistics
    stats: HypothesisStats,
}

/// Hypothesis testing statistics
#[derive(Debug, Default)]
pub struct HypothesisStats {
    pub total_tested: AtomicU64,
    pub succeeded: AtomicU64,
    pub failed: AtomicU64,
    pub cancelled: AtomicU64,
}

impl HypothesisTester {
    /// Create a new tester
    pub fn new(config: SpeculativeConfig) -> Self {
        Self {
            hypotheses: RwLock::new(HashMap::new()),
            config,
            stats: HypothesisStats::default(),
        }
    }

    /// Add a hypothesis
    pub fn add(&self, hypothesis: Hypothesis) -> bool {
        if !self.config.hypothesis_testing {
            return false;
        }

        if let Ok(mut hypotheses) = self.hypotheses.write() {
            if hypotheses.len() < self.config.max_hypotheses {
                hypotheses.insert(hypothesis.id.clone(), hypothesis);
                return true;
            }
        }
        false
    }

    /// Get a hypothesis by ID
    pub fn get(&self, id: &str) -> Option<Hypothesis> {
        self.hypotheses.read().ok()?.get(id).cloned()
    }

    /// Mark hypothesis as running
    pub fn start(&self, id: &str) {
        if let Ok(mut hypotheses) = self.hypotheses.write() {
            if let Some(h) = hypotheses.get_mut(id) {
                h.status = HypothesisStatus::Running;
            }
        }
    }

    /// Mark hypothesis as succeeded
    pub fn succeed(&self, id: &str, duration_ms: u64) {
        self.stats.total_tested.fetch_add(1, Ordering::Relaxed);
        self.stats.succeeded.fetch_add(1, Ordering::Relaxed);

        if let Ok(mut hypotheses) = self.hypotheses.write() {
            if let Some(h) = hypotheses.get_mut(id) {
                h.succeed(duration_ms);
            }
        }
    }

    /// Mark hypothesis as failed
    pub fn fail(&self, id: &str, duration_ms: u64, error: Option<String>) {
        self.stats.total_tested.fetch_add(1, Ordering::Relaxed);
        self.stats.failed.fetch_add(1, Ordering::Relaxed);

        if let Ok(mut hypotheses) = self.hypotheses.write() {
            if let Some(h) = hypotheses.get_mut(id) {
                h.fail(duration_ms, error);
            }
        }
    }

    /// Cancel a hypothesis
    pub fn cancel(&self, id: &str) {
        self.stats.cancelled.fetch_add(1, Ordering::Relaxed);

        if let Ok(mut hypotheses) = self.hypotheses.write() {
            if let Some(h) = hypotheses.get_mut(id) {
                h.cancel();
            }
        }
    }

    /// Cancel all pending hypotheses (when one succeeds)
    pub fn cancel_others(&self, except_id: &str) {
        if let Ok(mut hypotheses) = self.hypotheses.write() {
            for (id, h) in hypotheses.iter_mut() {
                if id != except_id && h.status == HypothesisStatus::Pending {
                    h.cancel();
                    self.stats.cancelled.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    /// Get pending hypotheses sorted by confidence
    pub fn pending(&self) -> Vec<Hypothesis> {
        let mut result: Vec<Hypothesis> = self
            .hypotheses
            .read()
            .ok()
            .map(|h| {
                h.values()
                    .filter(|h| h.status == HypothesisStatus::Pending)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        result.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        result
    }

    /// Get success rate
    pub fn success_rate(&self) -> f32 {
        let total = self.stats.total_tested.load(Ordering::Relaxed) as f32;
        let succeeded = self.stats.succeeded.load(Ordering::Relaxed) as f32;
        if total > 0.0 {
            succeeded / total
        } else {
            0.0
        }
    }

    /// Get summary
    pub fn summary(&self) -> HypothesisSummary {
        HypothesisSummary {
            total_tested: self.stats.total_tested.load(Ordering::Relaxed),
            succeeded: self.stats.succeeded.load(Ordering::Relaxed),
            failed: self.stats.failed.load(Ordering::Relaxed),
            cancelled: self.stats.cancelled.load(Ordering::Relaxed),
            success_rate: self.success_rate(),
        }
    }

    /// Clear all
    pub fn clear(&self) {
        if let Ok(mut hypotheses) = self.hypotheses.write() {
            hypotheses.clear();
        }
    }
}

impl Default for HypothesisTester {
    fn default() -> Self {
        Self::new(SpeculativeConfig::default())
    }
}

/// Summary of hypothesis testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HypothesisSummary {
    pub total_tested: u64,
    pub succeeded: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub success_rate: f32,
}

// ============================================================================
// Rollback Management
// ============================================================================

/// Manages rollback on misprediction
pub struct RollbackManager {
    /// Checkpoints for rollback
    checkpoints: RwLock<Vec<Checkpoint>>,
    /// Maximum checkpoints to keep
    max_checkpoints: usize,
    /// Statistics
    stats: RollbackStats,
}

/// A checkpoint for potential rollback
#[derive(Debug, Clone)]
pub struct Checkpoint {
    /// Unique identifier
    pub id: String,
    /// Description
    pub description: String,
    /// Timestamp
    pub timestamp: u64,
    /// State to restore
    pub state: serde_json::Value,
    /// Files modified since this checkpoint
    pub modified_files: Vec<FileState>,
}

/// State of a file at checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileState {
    /// File path
    pub path: String,
    /// Content at checkpoint
    pub content: String,
    /// Hash of content
    pub content_hash: u64,
}

/// Rollback statistics
#[derive(Debug, Default)]
pub struct RollbackStats {
    pub checkpoints_created: AtomicU64,
    pub rollbacks_performed: AtomicU64,
    pub rollbacks_avoided: AtomicU64,
}

impl RollbackManager {
    /// Create a new rollback manager
    pub fn new(max_checkpoints: usize) -> Self {
        Self {
            checkpoints: RwLock::new(Vec::new()),
            max_checkpoints,
            stats: RollbackStats::default(),
        }
    }

    /// Create a checkpoint
    pub fn checkpoint(&self, description: &str, state: serde_json::Value) -> String {
        let id = format!("ckpt_{}", uuid_v4());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let checkpoint = Checkpoint {
            id: id.clone(),
            description: description.to_string(),
            timestamp: now,
            state,
            modified_files: Vec::new(),
        };

        if let Ok(mut checkpoints) = self.checkpoints.write() {
            checkpoints.push(checkpoint);
            self.stats
                .checkpoints_created
                .fetch_add(1, Ordering::Relaxed);

            // Limit checkpoints
            while checkpoints.len() > self.max_checkpoints {
                checkpoints.remove(0);
            }
        }

        id
    }

    /// Record a file modification
    pub fn record_file(&self, checkpoint_id: &str, path: &str) {
        if let Ok(content) = std::fs::read_to_string(path) {
            let hash = simple_hash(&content);

            if let Ok(mut checkpoints) = self.checkpoints.write() {
                if let Some(ckpt) = checkpoints.iter_mut().find(|c| c.id == checkpoint_id) {
                    ckpt.modified_files.push(FileState {
                        path: path.to_string(),
                        content,
                        content_hash: hash,
                    });
                }
            }
        }
    }

    /// Rollback to a checkpoint
    pub fn rollback(&self, checkpoint_id: &str) -> Result<(), String> {
        let checkpoint = self
            .checkpoints
            .read()
            .ok()
            .and_then(|c| c.iter().find(|ckpt| ckpt.id == checkpoint_id).cloned())
            .ok_or_else(|| "Checkpoint not found".to_string())?;

        // Restore files
        for file_state in &checkpoint.modified_files {
            std::fs::write(&file_state.path, &file_state.content)
                .map_err(|e| format!("Failed to restore {}: {}", file_state.path, e))?;
        }

        self.stats
            .rollbacks_performed
            .fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Check if rollback is needed by comparing current state
    pub fn needs_rollback(&self, checkpoint_id: &str) -> bool {
        if let Some(checkpoint) = self
            .checkpoints
            .read()
            .ok()
            .and_then(|c| c.iter().find(|ckpt| ckpt.id == checkpoint_id).cloned())
        {
            for file_state in &checkpoint.modified_files {
                if let Ok(current) = std::fs::read_to_string(&file_state.path) {
                    if simple_hash(&current) != file_state.content_hash {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Discard a checkpoint (speculation was correct)
    pub fn discard(&self, checkpoint_id: &str) {
        self.stats.rollbacks_avoided.fetch_add(1, Ordering::Relaxed);

        if let Ok(mut checkpoints) = self.checkpoints.write() {
            checkpoints.retain(|c| c.id != checkpoint_id);
        }
    }

    /// Get summary
    pub fn summary(&self) -> RollbackSummary {
        RollbackSummary {
            checkpoints_created: self.stats.checkpoints_created.load(Ordering::Relaxed),
            rollbacks_performed: self.stats.rollbacks_performed.load(Ordering::Relaxed),
            rollbacks_avoided: self.stats.rollbacks_avoided.load(Ordering::Relaxed),
            active_checkpoints: self.checkpoints.read().map(|c| c.len()).unwrap_or(0),
        }
    }

    /// Clear all checkpoints
    pub fn clear(&self) {
        if let Ok(mut checkpoints) = self.checkpoints.write() {
            checkpoints.clear();
        }
    }
}

impl Default for RollbackManager {
    fn default() -> Self {
        Self::new(10)
    }
}

/// Summary of rollback management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackSummary {
    pub checkpoints_created: u64,
    pub rollbacks_performed: u64,
    pub rollbacks_avoided: u64,
    pub active_checkpoints: usize,
}

// ============================================================================
// Speculative Executor
// ============================================================================

/// Unified speculative executor
pub struct SpeculativeExecutor {
    /// Configuration
    config: SpeculativeConfig,
    /// Predictor
    predictor: Predictor,
    /// Prefetch manager
    prefetch: PrefetchManager,
    /// Cache warmer
    cache_warmer: CacheWarmer,
    /// Hypothesis tester
    hypothesis_tester: HypothesisTester,
    /// Rollback manager
    rollback: RollbackManager,
}

impl SpeculativeExecutor {
    /// Create a new executor
    pub fn new(config: SpeculativeConfig) -> Self {
        Self {
            predictor: Predictor::new(config.clone()),
            prefetch: PrefetchManager::new(config.clone()),
            cache_warmer: CacheWarmer::new(config.clone()),
            hypothesis_tester: HypothesisTester::new(config.clone()),
            rollback: RollbackManager::new(10),
            config,
        }
    }

    /// Record an action for learning
    pub fn record_action(&self, action: &str, context_hash: u64) {
        self.predictor.record_action(action, context_hash);
    }

    /// Predict and prepare next steps
    pub fn prepare_next(&self, current_action: &str, context_hash: u64) -> Vec<Prediction> {
        if !self.config.enabled {
            return Vec::new();
        }

        let predictions = self.predictor.predict(current_action, context_hash);

        // Schedule prefetches for file-related predictions
        for pred in &predictions {
            if pred.tool_name == "file_read" || pred.tool_name == "file_edit" {
                if let Some(path) = pred.arguments.get("path").and_then(|v| v.as_str()) {
                    self.prefetch.schedule(path);
                }
            }
        }

        predictions
    }

    /// Try to get prefetched content
    pub fn get_prefetched(&self, path: &str) -> Option<String> {
        self.prefetch.get(path)
    }

    /// Create a checkpoint before speculative execution
    pub fn create_checkpoint(&self, description: &str) -> String {
        self.rollback.checkpoint(description, serde_json::json!({}))
    }

    /// Record file for potential rollback
    pub fn record_file_for_rollback(&self, checkpoint_id: &str, path: &str) {
        self.rollback.record_file(checkpoint_id, path);
    }

    /// Rollback to checkpoint
    pub fn rollback(&self, checkpoint_id: &str) -> Result<(), String> {
        self.rollback.rollback(checkpoint_id)
    }

    /// Discard checkpoint (speculation was correct)
    pub fn confirm_speculation(&self, checkpoint_id: &str) {
        self.rollback.discard(checkpoint_id);
    }

    /// Get comprehensive summary
    pub fn summary(&self) -> SpeculativeSummary {
        SpeculativeSummary {
            prediction_accuracy: self.predictor.accuracy(),
            prefetch: self.prefetch.summary(),
            cache_warm: self.cache_warmer.summary(),
            hypothesis: self.hypothesis_tester.summary(),
            rollback: self.rollback.summary(),
        }
    }

    /// Execute pending operations
    pub fn execute_pending(&self) {
        self.prefetch.execute_prefetches();
        self.cache_warmer.execute_warms();
    }

    /// Clear all state
    pub fn clear(&self) {
        self.predictor.clear();
        self.prefetch.clear();
        self.cache_warmer.clear();
        self.hypothesis_tester.clear();
        self.rollback.clear();
    }
}

impl Default for SpeculativeExecutor {
    fn default() -> Self {
        Self::new(SpeculativeConfig::default())
    }
}

/// Comprehensive speculative execution summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeculativeSummary {
    pub prediction_accuracy: f32,
    pub prefetch: PrefetchSummary,
    pub cache_warm: WarmSummary,
    pub hypothesis: HypothesisSummary,
    pub rollback: RollbackSummary,
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Generate a simple UUID-like string
fn uuid_v4() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:x}{:x}", now.as_secs(), now.subsec_nanos())
}

/// Simple hash function for content
fn simple_hash(s: &str) -> u64 {
    let mut hash: u64 = 5381;
    for byte in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    hash
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_speculative_config_default() {
        let config = SpeculativeConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_speculative_ops, 10);
        assert_eq!(config.min_confidence, 0.7);
    }

    #[test]
    fn test_prediction_new() {
        let pred = Prediction::new("file_read", serde_json::json!({"path": "test.txt"}), 0.8);
        assert_eq!(pred.tool_name, "file_read");
        assert_eq!(pred.confidence, 0.8);
        assert!(pred.id.starts_with("pred_"));
    }

    #[test]
    fn test_prediction_with_reasoning() {
        let pred = Prediction::new("file_read", serde_json::json!({}), 0.8)
            .with_reasoning("Test reasoning");
        assert_eq!(pred.reasoning, "Test reasoning");
    }

    #[test]
    fn test_prediction_with_depth() {
        let pred = Prediction::new("file_read", serde_json::json!({}), 0.8).with_depth(2);
        assert_eq!(pred.depth, 2);
    }

    #[test]
    fn test_predictor_new() {
        let predictor = Predictor::default();
        assert_eq!(predictor.accuracy(), 0.0);
    }

    #[test]
    fn test_predictor_record_action() {
        let predictor = Predictor::default();
        predictor.record_action("file_read", 12345);
        predictor.record_action("file_edit", 12345);
        predictor.record_action("file_read", 12345);

        // Predictions may not be confident enough yet
        let predictions = predictor.predict("file_edit", 12345);
        // Just verify it doesn't panic
        assert!(predictions.len() <= predictor.config.max_speculative_ops);
    }

    #[test]
    fn test_prefetch_manager_schedule() {
        let prefetch = PrefetchManager::default();
        assert!(prefetch.schedule("/tmp/test.txt"));
    }

    #[test]
    fn test_prefetch_manager_get_miss() {
        let prefetch = PrefetchManager::default();
        assert!(prefetch.get("/nonexistent").is_none());
    }

    #[test]
    fn test_prefetch_summary() {
        let prefetch = PrefetchManager::default();
        let summary = prefetch.summary();
        assert_eq!(summary.total_prefetched, 0);
        assert_eq!(summary.hit_rate, 0.0);
    }

    #[test]
    fn test_cache_warmer_schedule() {
        let warmer = CacheWarmer::default();
        warmer.schedule("key1", serde_json::json!({"value": 1}), 10);
        warmer.execute_warms();

        let value = warmer.get("key1");
        assert!(value.is_some());
    }

    #[test]
    fn test_cache_warmer_get_miss() {
        let warmer = CacheWarmer::default();
        assert!(warmer.get("nonexistent").is_none());
    }

    #[test]
    fn test_cache_warmer_summary() {
        let warmer = CacheWarmer::default();
        let summary = warmer.summary();
        assert_eq!(summary.total_warmed, 0);
    }

    #[test]
    fn test_hypothesis_new() {
        let hyp = Hypothesis::new("Test hypothesis", 0.9);
        assert_eq!(hyp.description, "Test hypothesis");
        assert_eq!(hyp.confidence, 0.9);
        assert_eq!(hyp.status, HypothesisStatus::Pending);
    }

    #[test]
    fn test_hypothesis_add_action() {
        let mut hyp = Hypothesis::new("Test", 0.9);
        hyp.add_action("file_read", serde_json::json!({"path": "test.txt"}));
        hyp.add_action("file_edit", serde_json::json!({"path": "test.txt"}));

        assert_eq!(hyp.actions.len(), 2);
        assert_eq!(hyp.actions[0].order, 0);
        assert_eq!(hyp.actions[1].order, 1);
    }

    #[test]
    fn test_hypothesis_succeed() {
        let mut hyp = Hypothesis::new("Test", 0.9);
        hyp.succeed(100);

        assert_eq!(hyp.status, HypothesisStatus::Succeeded);
        assert!(hyp.result.is_some());
        assert!(hyp.result.as_ref().unwrap().correct);
    }

    #[test]
    fn test_hypothesis_fail() {
        let mut hyp = Hypothesis::new("Test", 0.9);
        hyp.fail(100, Some("Error".to_string()));

        assert_eq!(hyp.status, HypothesisStatus::Failed);
        assert!(!hyp.result.as_ref().unwrap().correct);
    }

    #[test]
    fn test_hypothesis_tester_add() {
        let tester = HypothesisTester::default();
        let hyp = Hypothesis::new("Test", 0.9);
        assert!(tester.add(hyp));
    }

    #[test]
    fn test_hypothesis_tester_pending() {
        let tester = HypothesisTester::default();
        tester.add(Hypothesis::new("Test 1", 0.9));
        tester.add(Hypothesis::new("Test 2", 0.7));

        let pending = tester.pending();
        assert_eq!(pending.len(), 2);
        // Should be sorted by confidence
        assert!(pending[0].confidence >= pending[1].confidence);
    }

    #[test]
    fn test_hypothesis_tester_succeed() {
        let tester = HypothesisTester::default();
        let hyp = Hypothesis::new("Test", 0.9);
        let id = hyp.id.clone();
        tester.add(hyp);

        tester.succeed(&id, 100);

        let summary = tester.summary();
        assert_eq!(summary.succeeded, 1);
    }

    #[test]
    fn test_rollback_manager_checkpoint() {
        let manager = RollbackManager::default();
        let id = manager.checkpoint("Test", serde_json::json!({}));
        assert!(id.starts_with("ckpt_"));
    }

    #[test]
    fn test_rollback_manager_discard() {
        let manager = RollbackManager::default();
        let id = manager.checkpoint("Test", serde_json::json!({}));
        manager.discard(&id);

        let summary = manager.summary();
        assert_eq!(summary.rollbacks_avoided, 1);
    }

    #[test]
    fn test_rollback_summary() {
        let manager = RollbackManager::default();
        manager.checkpoint("Test 1", serde_json::json!({}));
        manager.checkpoint("Test 2", serde_json::json!({}));

        let summary = manager.summary();
        assert_eq!(summary.checkpoints_created, 2);
        assert_eq!(summary.active_checkpoints, 2);
    }

    #[test]
    fn test_speculative_executor_new() {
        let executor = SpeculativeExecutor::default();
        let summary = executor.summary();
        assert_eq!(summary.prediction_accuracy, 0.0);
    }

    #[test]
    fn test_speculative_executor_record_action() {
        let executor = SpeculativeExecutor::default();
        executor.record_action("file_read", 12345);
        // Should not panic
    }

    #[test]
    fn test_speculative_executor_create_checkpoint() {
        let executor = SpeculativeExecutor::default();
        let id = executor.create_checkpoint("Test checkpoint");
        assert!(id.starts_with("ckpt_"));
    }

    #[test]
    fn test_speculative_executor_confirm() {
        let executor = SpeculativeExecutor::default();
        let id = executor.create_checkpoint("Test");
        executor.confirm_speculation(&id);

        let summary = executor.summary();
        assert_eq!(summary.rollback.rollbacks_avoided, 1);
    }

    #[test]
    fn test_speculative_executor_clear() {
        let executor = SpeculativeExecutor::default();
        executor.create_checkpoint("Test");
        executor.clear();

        let summary = executor.summary();
        assert_eq!(summary.rollback.active_checkpoints, 0);
    }

    #[test]
    fn test_uuid_v4() {
        let id1 = uuid_v4();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let id2 = uuid_v4();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_simple_hash() {
        let h1 = simple_hash("hello");
        let h2 = simple_hash("hello");
        let h3 = simple_hash("world");

        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_action_sequence() {
        let seq = ActionSequence {
            actions: vec!["file_read".to_string(), "file_edit".to_string()],
            timestamp: 12345,
            context_hash: 67890,
        };

        assert_eq!(seq.actions.len(), 2);
    }

    #[test]
    fn test_pattern_stats_default() {
        let stats = PatternStats::default();
        assert_eq!(stats.count, 0);
        assert_eq!(stats.successes, 0);
    }

    #[test]
    fn test_prefetched_file() {
        let file = PrefetchedFile {
            path: "/tmp/test.txt".to_string(),
            content: "Hello".to_string(),
            fetched_at: 12345,
            was_used: false,
        };

        assert_eq!(file.path, "/tmp/test.txt");
        assert!(!file.was_used);
    }

    #[test]
    fn test_warm_request() {
        let req = WarmRequest {
            key: "test_key".to_string(),
            value: serde_json::json!({"data": 123}),
            priority: 10,
        };

        assert_eq!(req.priority, 10);
    }

    #[test]
    fn test_file_state() {
        let state = FileState {
            path: "/tmp/test.txt".to_string(),
            content: "Hello".to_string(),
            content_hash: 12345,
        };

        assert_eq!(state.content_hash, 12345);
    }

    #[test]
    fn test_hypothesis_cancel() {
        let mut hyp = Hypothesis::new("Test", 0.9);
        hyp.cancel();
        assert_eq!(hyp.status, HypothesisStatus::Cancelled);
    }

    #[test]
    fn test_hypothesis_tester_cancel_others() {
        let tester = HypothesisTester::default();
        let h1 = Hypothesis::new("Test 1", 0.9);
        let h2 = Hypothesis::new("Test 2", 0.8);
        let id1 = h1.id.clone();
        tester.add(h1);
        tester.add(h2);

        tester.cancel_others(&id1);

        let pending = tester.pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, id1);
    }
}
