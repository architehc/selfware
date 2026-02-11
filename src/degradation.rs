//! Graceful Degradation Framework
//!
//! This module provides robustness through graceful degradation:
//! - Offline mode for working without API access
//! - Fallback models when primary is unavailable
//! - Partial results when full completion isn't possible
//! - Smart retry strategies with backoff
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                 Degradation Manager                         │
//! │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐   │
//! │  │ Offline       │  │ Fallback      │  │ Partial       │   │
//! │  │ Handler       │  │ Chain         │  │ Results       │   │
//! │  └───────────────┘  └───────────────┘  └───────────────┘   │
//! │           │                  │                  │           │
//! │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐   │
//! │  │ Retry         │  │ Circuit       │  │ Health        │   │
//! │  │ Strategy      │  │ Breaker       │  │ Monitor       │   │
//! │  └───────────────┘  └───────────────┘  └───────────────┘   │
//! └─────────────────────────────────────────────────────────────┘
//! ```

// Feature-gated module - dead_code lint disabled at crate level

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for graceful degradation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradationConfig {
    /// Enable offline mode fallback
    pub enable_offline_mode: bool,
    /// Enable model fallback
    pub enable_model_fallback: bool,
    /// Enable partial results
    pub enable_partial_results: bool,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Base delay for exponential backoff (ms)
    pub base_delay_ms: u64,
    /// Maximum delay for exponential backoff (ms)
    pub max_delay_ms: u64,
    /// Circuit breaker threshold (failures before opening)
    pub circuit_threshold: u32,
    /// Circuit breaker reset time (seconds)
    pub circuit_reset_secs: u64,
}

impl Default for DegradationConfig {
    fn default() -> Self {
        Self {
            enable_offline_mode: true,
            enable_model_fallback: true,
            enable_partial_results: true,
            max_retries: 3,
            base_delay_ms: 1000,
            max_delay_ms: 30000,
            circuit_threshold: 5,
            circuit_reset_secs: 60,
        }
    }
}

// ============================================================================
// Error Types
// ============================================================================

/// Types of failures that can be handled
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FailureType {
    /// Network connectivity issues
    Network,
    /// API rate limiting
    RateLimit,
    /// API timeout
    Timeout,
    /// Model unavailable
    ModelUnavailable,
    /// Authentication failure
    AuthFailure,
    /// Server error (5xx)
    ServerError,
    /// Invalid request (4xx)
    ClientError,
    /// Resource exhausted
    ResourceExhausted,
    /// Unknown error
    Unknown,
}

impl FailureType {
    /// Check if failure is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            FailureType::Network
                | FailureType::RateLimit
                | FailureType::Timeout
                | FailureType::ServerError
                | FailureType::ModelUnavailable
        )
    }

    /// Check if failure suggests switching models
    pub fn should_fallback(&self) -> bool {
        matches!(
            self,
            FailureType::ModelUnavailable | FailureType::RateLimit | FailureType::ResourceExhausted
        )
    }

    /// Check if failure suggests going offline
    pub fn suggests_offline(&self) -> bool {
        matches!(self, FailureType::Network | FailureType::AuthFailure)
    }
}

/// A recoverable failure with context
#[derive(Debug, Clone)]
pub struct RecoverableFailure {
    /// Type of failure
    pub failure_type: FailureType,
    /// Error message
    pub message: String,
    /// Suggested retry delay (ms)
    pub retry_after_ms: Option<u64>,
    /// Suggested fallback
    pub suggested_fallback: Option<String>,
    /// Timestamp
    pub timestamp: u64,
}

impl RecoverableFailure {
    pub fn new(failure_type: FailureType, message: &str) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            failure_type,
            message: message.to_string(),
            retry_after_ms: None,
            suggested_fallback: None,
            timestamp: now,
        }
    }

    pub fn with_retry_after(mut self, ms: u64) -> Self {
        self.retry_after_ms = Some(ms);
        self
    }

    pub fn with_fallback(mut self, fallback: &str) -> Self {
        self.suggested_fallback = Some(fallback.to_string());
        self
    }
}

// ============================================================================
// Retry Strategy
// ============================================================================

/// Retry strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryStrategy {
    /// Maximum attempts
    pub max_attempts: u32,
    /// Base delay for exponential backoff
    pub base_delay: Duration,
    /// Maximum delay
    pub max_delay: Duration,
    /// Jitter factor (0.0 - 1.0)
    pub jitter_factor: f32,
    /// Whether to retry on this failure type
    pub retry_on: Vec<FailureType>,
}

impl Default for RetryStrategy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            jitter_factor: 0.1,
            retry_on: vec![
                FailureType::Network,
                FailureType::Timeout,
                FailureType::ServerError,
                FailureType::RateLimit,
            ],
        }
    }
}

impl RetryStrategy {
    /// Calculate delay for attempt number
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }

        let base_ms = self.base_delay.as_millis() as u64;
        let exp_delay = base_ms * 2u64.pow(attempt - 1);
        let capped = exp_delay.min(self.max_delay.as_millis() as u64);

        // Add jitter
        let jitter_range = (capped as f32 * self.jitter_factor) as u64;
        let jitter = if jitter_range > 0 {
            (capped % jitter_range).min(jitter_range)
        } else {
            0
        };

        Duration::from_millis(capped + jitter)
    }

    /// Check if should retry this failure
    pub fn should_retry(&self, failure: &RecoverableFailure, attempt: u32) -> bool {
        if attempt >= self.max_attempts {
            return false;
        }

        self.retry_on.contains(&failure.failure_type)
    }
}

/// Retry executor
pub struct RetryExecutor {
    strategy: RetryStrategy,
    stats: RetryStats,
}

/// Retry statistics
#[derive(Debug, Default)]
pub struct RetryStats {
    pub total_attempts: AtomicU64,
    pub successful_retries: AtomicU64,
    pub failed_retries: AtomicU64,
    pub total_delay_ms: AtomicU64,
}

impl RetryExecutor {
    pub fn new(strategy: RetryStrategy) -> Self {
        Self {
            strategy,
            stats: RetryStats::default(),
        }
    }

    /// Execute with retry logic
    pub async fn execute<F, T, E>(&self, mut f: F) -> Result<T, RecoverableFailure>
    where
        F: FnMut() -> Result<T, RecoverableFailure>,
    {
        let mut last_failure = None;

        for attempt in 0..self.strategy.max_attempts {
            self.stats.total_attempts.fetch_add(1, Ordering::Relaxed);

            match f() {
                Ok(result) => {
                    if attempt > 0 {
                        self.stats
                            .successful_retries
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    return Ok(result);
                }
                Err(failure) => {
                    if !self.strategy.should_retry(&failure, attempt + 1) {
                        return Err(failure);
                    }

                    let delay = if let Some(retry_after) = failure.retry_after_ms {
                        Duration::from_millis(retry_after)
                    } else {
                        self.strategy.delay_for_attempt(attempt + 1)
                    };

                    self.stats
                        .total_delay_ms
                        .fetch_add(delay.as_millis() as u64, Ordering::Relaxed);
                    tokio::time::sleep(delay).await;

                    last_failure = Some(failure);
                }
            }
        }

        self.stats.failed_retries.fetch_add(1, Ordering::Relaxed);
        Err(last_failure.unwrap_or_else(|| {
            RecoverableFailure::new(FailureType::Unknown, "Max retries exceeded")
        }))
    }

    /// Get stats
    pub fn stats(&self) -> RetrySummary {
        RetrySummary {
            total_attempts: self.stats.total_attempts.load(Ordering::Relaxed),
            successful_retries: self.stats.successful_retries.load(Ordering::Relaxed),
            failed_retries: self.stats.failed_retries.load(Ordering::Relaxed),
            total_delay_ms: self.stats.total_delay_ms.load(Ordering::Relaxed),
        }
    }
}

impl Default for RetryExecutor {
    fn default() -> Self {
        Self::new(RetryStrategy::default())
    }
}

/// Retry summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrySummary {
    pub total_attempts: u64,
    pub successful_retries: u64,
    pub failed_retries: u64,
    pub total_delay_ms: u64,
}

// ============================================================================
// Circuit Breaker
// ============================================================================

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    /// Circuit is closed, requests flow normally
    Closed,
    /// Circuit is open, requests are blocked
    Open,
    /// Circuit is half-open, testing if service recovered
    HalfOpen,
}

/// Circuit breaker for preventing cascade failures
pub struct CircuitBreaker {
    /// Current state
    state: RwLock<CircuitState>,
    /// Failure count
    failure_count: AtomicU64,
    /// Success count (for half-open state)
    success_count: AtomicU64,
    /// Threshold to trip circuit
    threshold: u32,
    /// Time when circuit opened
    opened_at: RwLock<Option<Instant>>,
    /// Reset timeout
    reset_timeout: Duration,
    /// Statistics
    stats: CircuitStats,
}

/// Circuit breaker statistics
#[derive(Debug, Default)]
pub struct CircuitStats {
    pub times_opened: AtomicU64,
    pub times_closed: AtomicU64,
    pub rejected_requests: AtomicU64,
}

impl CircuitBreaker {
    pub fn new(threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            state: RwLock::new(CircuitState::Closed),
            failure_count: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            threshold,
            opened_at: RwLock::new(None),
            reset_timeout,
            stats: CircuitStats::default(),
        }
    }

    /// Check if request is allowed
    pub fn allow_request(&self) -> bool {
        let state = self
            .state
            .read()
            .map(|s| *s)
            .unwrap_or(CircuitState::Closed);

        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if reset timeout has passed
                if let Ok(opened_at) = self.opened_at.read() {
                    if let Some(opened) = *opened_at {
                        if opened.elapsed() >= self.reset_timeout {
                            // Transition to half-open
                            if let Ok(mut s) = self.state.write() {
                                *s = CircuitState::HalfOpen;
                            }
                            self.success_count.store(0, Ordering::Relaxed);
                            return true;
                        }
                    }
                }
                self.stats.rejected_requests.fetch_add(1, Ordering::Relaxed);
                false
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record a success
    pub fn record_success(&self) {
        let state = self
            .state
            .read()
            .map(|s| *s)
            .unwrap_or(CircuitState::Closed);

        match state {
            CircuitState::Closed => {
                self.failure_count.store(0, Ordering::Relaxed);
            }
            CircuitState::HalfOpen => {
                let successes = self.success_count.fetch_add(1, Ordering::Relaxed);
                if successes >= 2 {
                    // Close the circuit
                    if let Ok(mut s) = self.state.write() {
                        *s = CircuitState::Closed;
                    }
                    self.stats.times_closed.fetch_add(1, Ordering::Relaxed);
                    self.failure_count.store(0, Ordering::Relaxed);
                }
            }
            CircuitState::Open => {}
        }
    }

    /// Record a failure
    pub fn record_failure(&self) {
        let state = self
            .state
            .read()
            .map(|s| *s)
            .unwrap_or(CircuitState::Closed);

        match state {
            CircuitState::Closed => {
                let failures = self.failure_count.fetch_add(1, Ordering::Relaxed);
                if failures + 1 >= self.threshold as u64 {
                    // Open the circuit
                    if let Ok(mut s) = self.state.write() {
                        *s = CircuitState::Open;
                    }
                    if let Ok(mut opened) = self.opened_at.write() {
                        *opened = Some(Instant::now());
                    }
                    self.stats.times_opened.fetch_add(1, Ordering::Relaxed);
                }
            }
            CircuitState::HalfOpen => {
                // Back to open
                if let Ok(mut s) = self.state.write() {
                    *s = CircuitState::Open;
                }
                if let Ok(mut opened) = self.opened_at.write() {
                    *opened = Some(Instant::now());
                }
            }
            CircuitState::Open => {}
        }
    }

    /// Get current state
    pub fn state(&self) -> CircuitState {
        self.state
            .read()
            .map(|s| *s)
            .unwrap_or(CircuitState::Closed)
    }

    /// Reset the circuit
    pub fn reset(&self) {
        if let Ok(mut s) = self.state.write() {
            *s = CircuitState::Closed;
        }
        self.failure_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
    }

    /// Get stats summary
    pub fn summary(&self) -> CircuitSummary {
        CircuitSummary {
            state: self.state(),
            failure_count: self.failure_count.load(Ordering::Relaxed),
            times_opened: self.stats.times_opened.load(Ordering::Relaxed),
            times_closed: self.stats.times_closed.load(Ordering::Relaxed),
            rejected_requests: self.stats.rejected_requests.load(Ordering::Relaxed),
        }
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(5, Duration::from_secs(60))
    }
}

/// Circuit breaker summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitSummary {
    pub state: CircuitState,
    pub failure_count: u64,
    pub times_opened: u64,
    pub times_closed: u64,
    pub rejected_requests: u64,
}

// ============================================================================
// Fallback Chain
// ============================================================================

/// A fallback option
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackOption {
    /// Identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// Priority (lower = higher priority)
    pub priority: u32,
    /// Whether it's available
    pub available: bool,
    /// Capability level (1 = basic, 2 = standard, 3 = full)
    pub capability_level: u8,
}

/// Manages fallback chain for models/services
pub struct FallbackChain {
    /// Available options
    options: RwLock<Vec<FallbackOption>>,
    /// Current primary
    current: RwLock<Option<String>>,
    /// Disabled options
    disabled: RwLock<Vec<String>>,
    /// Statistics
    stats: FallbackStats,
}

/// Fallback statistics
#[derive(Debug, Default)]
pub struct FallbackStats {
    pub fallback_count: AtomicU64,
    pub recovery_count: AtomicU64,
}

impl FallbackChain {
    pub fn new() -> Self {
        Self {
            options: RwLock::new(Vec::new()),
            current: RwLock::new(None),
            disabled: RwLock::new(Vec::new()),
            stats: FallbackStats::default(),
        }
    }

    /// Add a fallback option
    pub fn add_option(&self, option: FallbackOption) {
        if let Ok(mut options) = self.options.write() {
            options.push(option);
            options.sort_by_key(|o| o.priority);
        }
    }

    /// Get current option
    pub fn current(&self) -> Option<FallbackOption> {
        let current_id = self.current.read().ok()?.clone()?;
        self.options
            .read()
            .ok()?
            .iter()
            .find(|o| o.id == current_id)
            .cloned()
    }

    /// Get next fallback option
    pub fn next_fallback(&self) -> Option<FallbackOption> {
        let disabled = self.disabled.read().ok()?;
        let current_id = self.current.read().ok()?.clone();

        self.options
            .read()
            .ok()?
            .iter()
            .filter(|o| o.available)
            .filter(|o| !disabled.contains(&o.id))
            .filter(|o| current_id.as_ref() != Some(&o.id))
            .min_by_key(|o| o.priority)
            .cloned()
    }

    /// Switch to fallback
    pub fn switch_to_fallback(&self) -> Option<FallbackOption> {
        if let Some(fallback) = self.next_fallback() {
            // Disable current
            if let Ok(current) = self.current.read() {
                if let Some(ref id) = *current {
                    if let Ok(mut disabled) = self.disabled.write() {
                        if !disabled.contains(id) {
                            disabled.push(id.clone());
                        }
                    }
                }
            }

            // Switch to fallback
            if let Ok(mut current) = self.current.write() {
                *current = Some(fallback.id.clone());
            }

            self.stats.fallback_count.fetch_add(1, Ordering::Relaxed);
            return Some(fallback);
        }
        None
    }

    /// Mark option as available again
    pub fn mark_available(&self, id: &str) {
        if let Ok(mut disabled) = self.disabled.write() {
            disabled.retain(|d| d != id);
        }
    }

    /// Try to recover to primary
    pub fn try_recover_primary(&self) -> bool {
        if let Ok(options) = self.options.read() {
            if let Some(primary) = options.first() {
                if let Ok(disabled) = self.disabled.read() {
                    if !disabled.contains(&primary.id) {
                        if let Ok(mut current) = self.current.write() {
                            if current.as_ref() != Some(&primary.id) {
                                *current = Some(primary.id.clone());
                                self.stats.recovery_count.fetch_add(1, Ordering::Relaxed);
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Get summary
    pub fn summary(&self) -> FallbackSummary {
        let options = self.options.read().map(|o| o.len()).unwrap_or(0);
        let disabled = self.disabled.read().map(|d| d.len()).unwrap_or(0);

        FallbackSummary {
            total_options: options,
            available_options: options - disabled,
            disabled_options: disabled,
            current: self.current().map(|c| c.id),
            fallback_count: self.stats.fallback_count.load(Ordering::Relaxed),
            recovery_count: self.stats.recovery_count.load(Ordering::Relaxed),
        }
    }
}

impl Default for FallbackChain {
    fn default() -> Self {
        Self::new()
    }
}

/// Fallback chain summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackSummary {
    pub total_options: usize,
    pub available_options: usize,
    pub disabled_options: usize,
    pub current: Option<String>,
    pub fallback_count: u64,
    pub recovery_count: u64,
}

// ============================================================================
// Offline Mode
// ============================================================================

/// Offline mode handler
pub struct OfflineHandler {
    /// Whether offline mode is enabled
    enabled: AtomicBool,
    /// Whether currently offline
    is_offline: AtomicBool,
    /// Cached responses for offline use
    cache: RwLock<HashMap<String, CachedResponse>>,
    /// Statistics
    stats: OfflineStats,
}

/// Cached response for offline use
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedResponse {
    /// Cache key
    pub key: String,
    /// Response content
    pub content: String,
    /// Cached at timestamp
    pub cached_at: u64,
    /// Times used
    pub use_count: u32,
}

/// Offline mode statistics
#[derive(Debug, Default)]
pub struct OfflineStats {
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub times_went_offline: AtomicU64,
    pub times_went_online: AtomicU64,
}

impl OfflineHandler {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            is_offline: AtomicBool::new(false),
            cache: RwLock::new(HashMap::new()),
            stats: OfflineStats::default(),
        }
    }

    /// Check if offline mode is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Check if currently offline
    pub fn is_offline(&self) -> bool {
        self.is_offline.load(Ordering::Relaxed)
    }

    /// Go offline
    pub fn go_offline(&self) {
        if !self.is_offline.swap(true, Ordering::Relaxed) {
            self.stats
                .times_went_offline
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Go online
    pub fn go_online(&self) {
        if self.is_offline.swap(false, Ordering::Relaxed) {
            self.stats.times_went_online.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Cache a response
    pub fn cache(&self, key: &str, content: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Ok(mut cache) = self.cache.write() {
            cache.insert(
                key.to_string(),
                CachedResponse {
                    key: key.to_string(),
                    content: content.to_string(),
                    cached_at: now,
                    use_count: 0,
                },
            );
        }
    }

    /// Get cached response
    pub fn get_cached(&self, key: &str) -> Option<String> {
        if let Ok(mut cache) = self.cache.write() {
            if let Some(entry) = cache.get_mut(key) {
                entry.use_count += 1;
                self.stats.cache_hits.fetch_add(1, Ordering::Relaxed);
                return Some(entry.content.clone());
            }
        }
        self.stats.cache_misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Clear cache
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
    }

    /// Get summary
    pub fn summary(&self) -> OfflineSummary {
        let cache_size = self.cache.read().map(|c| c.len()).unwrap_or(0);

        OfflineSummary {
            enabled: self.is_enabled(),
            is_offline: self.is_offline(),
            cache_size,
            cache_hits: self.stats.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.stats.cache_misses.load(Ordering::Relaxed),
            times_went_offline: self.stats.times_went_offline.load(Ordering::Relaxed),
            times_went_online: self.stats.times_went_online.load(Ordering::Relaxed),
        }
    }
}

impl Default for OfflineHandler {
    fn default() -> Self {
        Self::new(true)
    }
}

/// Offline mode summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflineSummary {
    pub enabled: bool,
    pub is_offline: bool,
    pub cache_size: usize,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub times_went_offline: u64,
    pub times_went_online: u64,
}

// ============================================================================
// Partial Results
// ============================================================================

/// Partial result with completion percentage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialResult<T> {
    /// The partial data
    pub data: T,
    /// Completion percentage (0.0 - 1.0)
    pub completion: f32,
    /// Missing parts description
    pub missing: Vec<String>,
    /// Whether result is usable
    pub is_usable: bool,
    /// Quality score (0.0 - 1.0)
    pub quality: f32,
}

impl<T> PartialResult<T> {
    pub fn complete(data: T) -> Self {
        Self {
            data,
            completion: 1.0,
            missing: Vec::new(),
            is_usable: true,
            quality: 1.0,
        }
    }

    pub fn partial(data: T, completion: f32, missing: Vec<String>) -> Self {
        let is_usable = completion >= 0.5;
        let quality = completion * 0.8 + 0.2; // Partial quality reduction

        Self {
            data,
            completion,
            missing,
            is_usable,
            quality,
        }
    }

    pub fn failed(data: T, reason: &str) -> Self {
        Self {
            data,
            completion: 0.0,
            missing: vec![reason.to_string()],
            is_usable: false,
            quality: 0.0,
        }
    }
}

/// Manager for partial results
pub struct PartialResultManager {
    /// Minimum completion for usable result
    min_completion: f32,
    /// Statistics
    stats: PartialStats,
}

/// Partial result statistics
#[derive(Debug, Default)]
pub struct PartialStats {
    pub complete_results: AtomicU64,
    pub partial_results: AtomicU64,
    pub failed_results: AtomicU64,
    pub total_completion_pct: AtomicU64, // Stored as percentage * 100
}

impl PartialResultManager {
    pub fn new(min_completion: f32) -> Self {
        Self {
            min_completion: min_completion.clamp(0.0, 1.0),
            stats: PartialStats::default(),
        }
    }

    /// Record a result
    pub fn record<T>(&self, result: &PartialResult<T>) {
        let pct = (result.completion * 100.0) as u64;
        self.stats
            .total_completion_pct
            .fetch_add(pct, Ordering::Relaxed);

        if result.completion >= 1.0 {
            self.stats.complete_results.fetch_add(1, Ordering::Relaxed);
        } else if result.is_usable {
            self.stats.partial_results.fetch_add(1, Ordering::Relaxed);
        } else {
            self.stats.failed_results.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Check if a result meets the minimum completion threshold
    pub fn is_acceptable<T>(&self, result: &PartialResult<T>) -> bool {
        result.completion >= self.min_completion
    }

    /// Get the minimum completion threshold
    pub fn min_completion(&self) -> f32 {
        self.min_completion
    }

    /// Get summary
    pub fn summary(&self) -> PartialSummary {
        let complete = self.stats.complete_results.load(Ordering::Relaxed);
        let partial = self.stats.partial_results.load(Ordering::Relaxed);
        let failed = self.stats.failed_results.load(Ordering::Relaxed);
        let total = complete + partial + failed;

        let avg_completion = if total > 0 {
            self.stats.total_completion_pct.load(Ordering::Relaxed) as f32 / (total * 100) as f32
        } else {
            0.0
        };

        PartialSummary {
            complete_results: complete,
            partial_results: partial,
            failed_results: failed,
            average_completion: avg_completion,
            success_rate: if total > 0 {
                (complete + partial) as f32 / total as f32
            } else {
                0.0
            },
        }
    }
}

impl Default for PartialResultManager {
    fn default() -> Self {
        Self::new(0.5)
    }
}

/// Partial results summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialSummary {
    pub complete_results: u64,
    pub partial_results: u64,
    pub failed_results: u64,
    pub average_completion: f32,
    pub success_rate: f32,
}

// ============================================================================
// Health Monitor
// ============================================================================

/// Service health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Service name
    pub service: String,
    /// Status
    pub status: HealthStatus,
    /// Response time in ms
    pub response_time_ms: Option<u64>,
    /// Last check timestamp
    pub last_check: u64,
    /// Error message if unhealthy
    pub error: Option<String>,
}

/// Health monitor for tracking service health
pub struct HealthMonitor {
    /// Health checks
    checks: RwLock<HashMap<String, HealthCheck>>,
    /// Check interval
    check_interval: Duration,
    /// Unhealthy threshold (consecutive failures)
    unhealthy_threshold: u32,
    /// Failure counts
    failure_counts: RwLock<HashMap<String, u32>>,
}

impl HealthMonitor {
    pub fn new(check_interval: Duration, unhealthy_threshold: u32) -> Self {
        Self {
            checks: RwLock::new(HashMap::new()),
            check_interval,
            unhealthy_threshold,
            failure_counts: RwLock::new(HashMap::new()),
        }
    }

    /// Record a health check
    pub fn record_check(
        &self,
        service: &str,
        success: bool,
        response_time_ms: Option<u64>,
        error: Option<String>,
    ) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Update failure count
        let status = if success {
            if let Ok(mut counts) = self.failure_counts.write() {
                counts.remove(service);
            }

            // Determine health based on response time
            match response_time_ms {
                Some(ms) if ms > 5000 => HealthStatus::Degraded,
                _ => HealthStatus::Healthy,
            }
        } else {
            let failures = if let Ok(mut counts) = self.failure_counts.write() {
                let count = counts.entry(service.to_string()).or_default();
                *count += 1;
                *count
            } else {
                1
            };

            if failures >= self.unhealthy_threshold {
                HealthStatus::Unhealthy
            } else {
                HealthStatus::Degraded
            }
        };

        if let Ok(mut checks) = self.checks.write() {
            checks.insert(
                service.to_string(),
                HealthCheck {
                    service: service.to_string(),
                    status,
                    response_time_ms,
                    last_check: now,
                    error,
                },
            );
        }
    }

    /// Get the configured check interval
    pub fn check_interval(&self) -> Duration {
        self.check_interval
    }

    /// Check if a service needs a health check based on the interval
    pub fn needs_check(&self, service: &str) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.checks
            .read()
            .ok()
            .and_then(|c| c.get(service).map(|h| {
                now.saturating_sub(h.last_check) >= self.check_interval.as_secs()
            }))
            .unwrap_or(true) // If not found, needs check
    }

    /// Get health status for a service
    pub fn get_status(&self, service: &str) -> HealthStatus {
        self.checks
            .read()
            .ok()
            .and_then(|c| c.get(service).map(|h| h.status))
            .unwrap_or(HealthStatus::Unknown)
    }

    /// Get all health checks
    pub fn all_checks(&self) -> Vec<HealthCheck> {
        self.checks
            .read()
            .map(|c| c.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Get overall health
    pub fn overall_health(&self) -> HealthStatus {
        let checks = self.all_checks();
        if checks.is_empty() {
            return HealthStatus::Unknown;
        }

        if checks.iter().any(|c| c.status == HealthStatus::Unhealthy) {
            HealthStatus::Unhealthy
        } else if checks.iter().any(|c| c.status == HealthStatus::Degraded) {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        }
    }
}

impl Default for HealthMonitor {
    fn default() -> Self {
        Self::new(Duration::from_secs(30), 3)
    }
}

// ============================================================================
// Degradation Manager
// ============================================================================

/// Unified degradation manager
pub struct DegradationManager {
    config: DegradationConfig,
    retry_executor: RetryExecutor,
    circuit_breaker: CircuitBreaker,
    fallback_chain: FallbackChain,
    offline_handler: OfflineHandler,
    partial_manager: PartialResultManager,
    health_monitor: HealthMonitor,
}

impl DegradationManager {
    pub fn new(config: DegradationConfig) -> Self {
        let retry_strategy = RetryStrategy {
            max_attempts: config.max_retries,
            base_delay: Duration::from_millis(config.base_delay_ms),
            max_delay: Duration::from_millis(config.max_delay_ms),
            ..Default::default()
        };

        Self {
            retry_executor: RetryExecutor::new(retry_strategy),
            circuit_breaker: CircuitBreaker::new(
                config.circuit_threshold,
                Duration::from_secs(config.circuit_reset_secs),
            ),
            fallback_chain: FallbackChain::new(),
            offline_handler: OfflineHandler::new(config.enable_offline_mode),
            partial_manager: PartialResultManager::default(),
            health_monitor: HealthMonitor::default(),
            config,
        }
    }

    /// Handle a failure
    pub fn handle_failure(&self, failure: &RecoverableFailure) -> DegradationAction {
        // Record in circuit breaker
        self.circuit_breaker.record_failure();

        // Check if should go offline
        if failure.failure_type.suggests_offline() && self.config.enable_offline_mode {
            self.offline_handler.go_offline();
            return DegradationAction::GoOffline;
        }

        // Check if should fallback
        if failure.failure_type.should_fallback() && self.config.enable_model_fallback {
            if let Some(fallback) = self.fallback_chain.switch_to_fallback() {
                return DegradationAction::SwitchFallback(fallback.id);
            }
        }

        // Check if should retry
        if failure.failure_type.is_retryable() {
            return DegradationAction::Retry;
        }

        DegradationAction::Fail
    }

    /// Handle a success
    pub fn handle_success(&self) {
        self.circuit_breaker.record_success();
        self.offline_handler.go_online();
    }

    /// Check if request is allowed
    pub fn allow_request(&self) -> bool {
        self.circuit_breaker.allow_request()
    }

    /// Get components
    pub fn retry_executor(&self) -> &RetryExecutor {
        &self.retry_executor
    }

    pub fn circuit_breaker(&self) -> &CircuitBreaker {
        &self.circuit_breaker
    }

    pub fn fallback_chain(&self) -> &FallbackChain {
        &self.fallback_chain
    }

    pub fn offline_handler(&self) -> &OfflineHandler {
        &self.offline_handler
    }

    pub fn partial_manager(&self) -> &PartialResultManager {
        &self.partial_manager
    }

    pub fn health_monitor(&self) -> &HealthMonitor {
        &self.health_monitor
    }

    /// Get comprehensive summary
    pub fn summary(&self) -> DegradationSummary {
        DegradationSummary {
            retry: self.retry_executor.stats(),
            circuit: self.circuit_breaker.summary(),
            fallback: self.fallback_chain.summary(),
            offline: self.offline_handler.summary(),
            partial: self.partial_manager.summary(),
            overall_health: self.health_monitor.overall_health(),
        }
    }
}

impl Default for DegradationManager {
    fn default() -> Self {
        Self::new(DegradationConfig::default())
    }
}

/// Action to take after degradation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DegradationAction {
    /// Retry the request
    Retry,
    /// Switch to fallback
    SwitchFallback(String),
    /// Go to offline mode
    GoOffline,
    /// Fail completely
    Fail,
}

/// Comprehensive degradation summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradationSummary {
    pub retry: RetrySummary,
    pub circuit: CircuitSummary,
    pub fallback: FallbackSummary,
    pub offline: OfflineSummary,
    pub partial: PartialSummary,
    pub overall_health: HealthStatus,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_degradation_config_default() {
        let config = DegradationConfig::default();
        assert!(config.enable_offline_mode);
        assert!(config.enable_model_fallback);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_failure_type_retryable() {
        assert!(FailureType::Network.is_retryable());
        assert!(FailureType::Timeout.is_retryable());
        assert!(!FailureType::AuthFailure.is_retryable());
    }

    #[test]
    fn test_failure_type_should_fallback() {
        assert!(FailureType::ModelUnavailable.should_fallback());
        assert!(FailureType::RateLimit.should_fallback());
        assert!(!FailureType::Network.should_fallback());
    }

    #[test]
    fn test_failure_type_suggests_offline() {
        assert!(FailureType::Network.suggests_offline());
        assert!(FailureType::AuthFailure.suggests_offline());
        assert!(!FailureType::RateLimit.suggests_offline());
    }

    #[test]
    fn test_recoverable_failure_new() {
        let failure = RecoverableFailure::new(FailureType::Network, "Connection failed");
        assert_eq!(failure.failure_type, FailureType::Network);
        assert_eq!(failure.message, "Connection failed");
    }

    #[test]
    fn test_retry_strategy_delay() {
        let strategy = RetryStrategy::default();

        let d1 = strategy.delay_for_attempt(1);
        let d2 = strategy.delay_for_attempt(2);
        let d3 = strategy.delay_for_attempt(3);

        assert!(d1 < d2);
        assert!(d2 < d3);
    }

    #[test]
    fn test_retry_strategy_should_retry() {
        let strategy = RetryStrategy::default();

        let network_failure = RecoverableFailure::new(FailureType::Network, "err");
        assert!(strategy.should_retry(&network_failure, 0));
        assert!(!strategy.should_retry(&network_failure, 3));

        let auth_failure = RecoverableFailure::new(FailureType::AuthFailure, "err");
        assert!(!strategy.should_retry(&auth_failure, 0));
    }

    #[test]
    fn test_circuit_breaker_closed() {
        let cb = CircuitBreaker::default();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.allow_request());
    }

    #[test]
    fn test_circuit_breaker_opens() {
        let cb = CircuitBreaker::new(2, Duration::from_secs(60));

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.allow_request());
    }

    #[test]
    fn test_circuit_breaker_reset() {
        let cb = CircuitBreaker::new(1, Duration::from_secs(60));
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        cb.reset();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_fallback_chain_add() {
        let chain = FallbackChain::new();
        chain.add_option(FallbackOption {
            id: "primary".to_string(),
            name: "Primary Model".to_string(),
            priority: 1,
            available: true,
            capability_level: 3,
        });

        let summary = chain.summary();
        assert_eq!(summary.total_options, 1);
    }

    #[test]
    fn test_fallback_chain_switch() {
        let chain = FallbackChain::new();
        chain.add_option(FallbackOption {
            id: "primary".to_string(),
            name: "Primary".to_string(),
            priority: 1,
            available: true,
            capability_level: 3,
        });
        chain.add_option(FallbackOption {
            id: "secondary".to_string(),
            name: "Secondary".to_string(),
            priority: 2,
            available: true,
            capability_level: 2,
        });

        // Set current to primary
        if let Ok(mut current) = chain.current.write() {
            *current = Some("primary".to_string());
        }

        let fallback = chain.switch_to_fallback();
        assert!(fallback.is_some());
        assert_eq!(fallback.unwrap().id, "secondary");
    }

    #[test]
    fn test_offline_handler_toggle() {
        let handler = OfflineHandler::default();
        assert!(!handler.is_offline());

        handler.go_offline();
        assert!(handler.is_offline());

        handler.go_online();
        assert!(!handler.is_offline());
    }

    #[test]
    fn test_offline_handler_cache() {
        let handler = OfflineHandler::default();
        handler.cache("key1", "value1");

        let cached = handler.get_cached("key1");
        assert_eq!(cached, Some("value1".to_string()));

        let missing = handler.get_cached("key2");
        assert!(missing.is_none());
    }

    #[test]
    fn test_partial_result_complete() {
        let result: PartialResult<String> = PartialResult::complete("done".to_string());
        assert_eq!(result.completion, 1.0);
        assert!(result.is_usable);
        assert!(result.missing.is_empty());
    }

    #[test]
    fn test_partial_result_partial() {
        let result: PartialResult<String> =
            PartialResult::partial("half".to_string(), 0.5, vec!["second half".to_string()]);
        assert_eq!(result.completion, 0.5);
        assert!(result.is_usable);
        assert!(!result.missing.is_empty());
    }

    #[test]
    fn test_partial_result_failed() {
        let result: PartialResult<String> = PartialResult::failed("".to_string(), "error");
        assert_eq!(result.completion, 0.0);
        assert!(!result.is_usable);
    }

    #[test]
    fn test_health_monitor_record() {
        let monitor = HealthMonitor::default();
        monitor.record_check("api", true, Some(100), None);

        assert_eq!(monitor.get_status("api"), HealthStatus::Healthy);
    }

    #[test]
    fn test_health_monitor_degraded() {
        let monitor = HealthMonitor::default();
        monitor.record_check("api", true, Some(6000), None);

        assert_eq!(monitor.get_status("api"), HealthStatus::Degraded);
    }

    #[test]
    fn test_health_monitor_unhealthy() {
        let monitor = HealthMonitor::new(Duration::from_secs(30), 2);
        monitor.record_check("api", false, None, Some("error".to_string()));
        monitor.record_check("api", false, None, Some("error".to_string()));

        assert_eq!(monitor.get_status("api"), HealthStatus::Unhealthy);
    }

    #[test]
    fn test_degradation_manager_allow_request() {
        let manager = DegradationManager::default();
        assert!(manager.allow_request());
    }

    #[test]
    fn test_degradation_manager_handle_success() {
        let manager = DegradationManager::default();
        manager.handle_success();
        // Should not panic
    }

    #[test]
    fn test_degradation_action_retry() {
        let manager = DegradationManager::default();
        let failure = RecoverableFailure::new(FailureType::Timeout, "timeout");

        let action = manager.handle_failure(&failure);
        assert_eq!(action, DegradationAction::Retry);
    }

    #[test]
    fn test_degradation_action_offline() {
        let manager = DegradationManager::default();
        let failure = RecoverableFailure::new(FailureType::Network, "no network");

        let action = manager.handle_failure(&failure);
        assert_eq!(action, DegradationAction::GoOffline);
    }

    #[test]
    fn test_degradation_summary() {
        let manager = DegradationManager::default();
        let summary = manager.summary();

        assert_eq!(summary.circuit.state, CircuitState::Closed);
        assert!(!summary.offline.is_offline);
    }

    // Additional comprehensive tests

    #[test]
    fn test_degradation_config_serialize() {
        let config = DegradationConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("enable_offline_mode"));

        let parsed: DegradationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.max_retries, config.max_retries);
    }

    #[test]
    fn test_degradation_config_fields() {
        let config = DegradationConfig {
            enable_offline_mode: false,
            enable_model_fallback: false,
            enable_partial_results: false,
            max_retries: 5,
            base_delay_ms: 500,
            max_delay_ms: 10000,
            circuit_threshold: 10,
            circuit_reset_secs: 120,
        };

        assert!(!config.enable_offline_mode);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.base_delay_ms, 500);
    }

    #[test]
    fn test_failure_type_all_variants() {
        let variants = [
            FailureType::Network,
            FailureType::RateLimit,
            FailureType::Timeout,
            FailureType::ModelUnavailable,
            FailureType::AuthFailure,
            FailureType::ServerError,
            FailureType::ClientError,
            FailureType::ResourceExhausted,
            FailureType::Unknown,
        ];

        for v in &variants {
            let _ = format!("{:?}", v);
        }
    }

    #[test]
    fn test_failure_type_clone() {
        let ft = FailureType::Network;
        let cloned = ft;
        assert_eq!(cloned, ft);
    }

    #[test]
    fn test_failure_type_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(FailureType::Network);
        set.insert(FailureType::Timeout);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_failure_type_serialize() {
        let ft = FailureType::RateLimit;
        let json = serde_json::to_string(&ft).unwrap();
        let parsed: FailureType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ft);
    }

    #[test]
    fn test_failure_type_server_error() {
        assert!(FailureType::ServerError.is_retryable());
        assert!(!FailureType::ServerError.should_fallback());
        assert!(!FailureType::ServerError.suggests_offline());
    }

    #[test]
    fn test_failure_type_client_error() {
        assert!(!FailureType::ClientError.is_retryable());
        assert!(!FailureType::ClientError.should_fallback());
        assert!(!FailureType::ClientError.suggests_offline());
    }

    #[test]
    fn test_failure_type_resource_exhausted() {
        assert!(!FailureType::ResourceExhausted.is_retryable());
        assert!(FailureType::ResourceExhausted.should_fallback());
        assert!(!FailureType::ResourceExhausted.suggests_offline());
    }

    #[test]
    fn test_failure_type_unknown() {
        assert!(!FailureType::Unknown.is_retryable());
        assert!(!FailureType::Unknown.should_fallback());
        assert!(!FailureType::Unknown.suggests_offline());
    }

    #[test]
    fn test_recoverable_failure_with_retry_after() {
        let failure =
            RecoverableFailure::new(FailureType::RateLimit, "Rate limited").with_retry_after(5000);
        assert_eq!(failure.retry_after_ms, Some(5000));
    }

    #[test]
    fn test_recoverable_failure_with_fallback() {
        let failure = RecoverableFailure::new(FailureType::ModelUnavailable, "Model down")
            .with_fallback("backup-model");
        assert_eq!(failure.suggested_fallback, Some("backup-model".to_string()));
    }

    #[test]
    fn test_recoverable_failure_clone() {
        let failure = RecoverableFailure::new(FailureType::Network, "err");
        let cloned = failure.clone();
        assert_eq!(cloned.message, failure.message);
    }

    #[test]
    fn test_retry_strategy_default() {
        let strategy = RetryStrategy::default();
        assert_eq!(strategy.max_attempts, 3);
        assert!(strategy.jitter_factor >= 0.0 && strategy.jitter_factor <= 1.0);
    }

    #[test]
    fn test_retry_strategy_serialize() {
        let strategy = RetryStrategy::default();
        let json = serde_json::to_string(&strategy).unwrap();
        let parsed: RetryStrategy = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.max_attempts, strategy.max_attempts);
    }

    #[test]
    fn test_retry_strategy_zero_attempt() {
        let strategy = RetryStrategy::default();
        let delay = strategy.delay_for_attempt(0);
        assert_eq!(delay, Duration::ZERO);
    }

    #[test]
    fn test_retry_strategy_max_delay_cap() {
        let strategy = RetryStrategy {
            max_attempts: 10,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(5),
            jitter_factor: 0.0, // No jitter for deterministic test
            retry_on: vec![],
        };

        let delay = strategy.delay_for_attempt(10);
        // Should be capped at max_delay
        assert!(delay <= Duration::from_secs(5) + Duration::from_millis(100));
    }

    #[test]
    fn test_retry_executor_default() {
        let executor = RetryExecutor::default();
        let stats = executor.stats();
        assert_eq!(stats.total_attempts, 0);
    }

    #[test]
    fn test_retry_stats_default() {
        let stats = RetryStats::default();
        assert_eq!(stats.total_attempts.load(Ordering::Relaxed), 0);
        assert_eq!(stats.successful_retries.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_retry_summary_fields() {
        let summary = RetrySummary {
            total_attempts: 10,
            successful_retries: 3,
            failed_retries: 2,
            total_delay_ms: 5000,
        };

        assert_eq!(summary.total_attempts, 10);
        assert_eq!(summary.successful_retries, 3);
    }

    #[test]
    fn test_retry_summary_serialize() {
        let summary = RetrySummary {
            total_attempts: 5,
            successful_retries: 2,
            failed_retries: 1,
            total_delay_ms: 3000,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let parsed: RetrySummary = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.total_attempts, 5);
    }

    #[test]
    fn test_circuit_state_all_variants() {
        assert_eq!(CircuitState::Closed, CircuitState::Closed);
        assert_eq!(CircuitState::Open, CircuitState::Open);
        assert_eq!(CircuitState::HalfOpen, CircuitState::HalfOpen);
    }

    #[test]
    fn test_circuit_state_serialize() {
        let state = CircuitState::HalfOpen;
        let json = serde_json::to_string(&state).unwrap();
        let parsed: CircuitState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, state);
    }

    #[test]
    fn test_circuit_stats_default() {
        let stats = CircuitStats::default();
        assert_eq!(stats.times_opened.load(Ordering::Relaxed), 0);
        assert_eq!(stats.times_closed.load(Ordering::Relaxed), 0);
        assert_eq!(stats.rejected_requests.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_circuit_breaker_summary() {
        let cb = CircuitBreaker::default();
        let summary = cb.summary();
        assert_eq!(summary.state, CircuitState::Closed);
        assert_eq!(summary.failure_count, 0);
    }

    #[test]
    fn test_circuit_summary_serialize() {
        let summary = CircuitSummary {
            state: CircuitState::Open,
            failure_count: 5,
            times_opened: 1,
            times_closed: 0,
            rejected_requests: 10,
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("Open"));
    }

    #[test]
    fn test_circuit_breaker_success_resets_count() {
        let cb = CircuitBreaker::new(5, Duration::from_secs(60));
        cb.record_failure();
        cb.record_failure();
        cb.record_success();

        let summary = cb.summary();
        assert_eq!(summary.failure_count, 0);
    }

    #[test]
    fn test_fallback_option_struct() {
        let option = FallbackOption {
            id: "test".to_string(),
            name: "Test Model".to_string(),
            priority: 1,
            available: true,
            capability_level: 3,
        };

        assert_eq!(option.id, "test");
        assert_eq!(option.priority, 1);
        assert_eq!(option.capability_level, 3);
    }

    #[test]
    fn test_fallback_option_serialize() {
        let option = FallbackOption {
            id: "model".to_string(),
            name: "Model".to_string(),
            priority: 1,
            available: true,
            capability_level: 2,
        };
        let json = serde_json::to_string(&option).unwrap();
        let parsed: FallbackOption = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, option.id);
    }

    #[test]
    fn test_fallback_stats_default() {
        let stats = FallbackStats::default();
        assert_eq!(stats.fallback_count.load(Ordering::Relaxed), 0);
        assert_eq!(stats.recovery_count.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_fallback_chain_default() {
        let chain = FallbackChain::default();
        assert!(chain.current().is_none());
    }

    #[test]
    fn test_fallback_chain_next_fallback_none() {
        let chain = FallbackChain::new();
        assert!(chain.next_fallback().is_none());
    }

    #[test]
    fn test_fallback_chain_mark_available() {
        let chain = FallbackChain::new();
        chain.add_option(FallbackOption {
            id: "test".to_string(),
            name: "Test".to_string(),
            priority: 1,
            available: true,
            capability_level: 1,
        });

        // Disable and re-enable
        if let Ok(mut disabled) = chain.disabled.write() {
            disabled.push("test".to_string());
        }

        chain.mark_available("test");

        // Should be available again
        let disabled = chain.disabled.read().unwrap();
        assert!(!disabled.contains(&"test".to_string()));
    }

    #[test]
    fn test_fallback_chain_try_recover_primary() {
        let chain = FallbackChain::new();
        chain.add_option(FallbackOption {
            id: "primary".to_string(),
            name: "Primary".to_string(),
            priority: 1,
            available: true,
            capability_level: 3,
        });

        // Set current to something else
        if let Ok(mut current) = chain.current.write() {
            *current = Some("other".to_string());
        }

        let recovered = chain.try_recover_primary();
        assert!(recovered);
    }

    #[test]
    fn test_fallback_summary_serialize() {
        let summary = FallbackSummary {
            total_options: 3,
            available_options: 2,
            disabled_options: 1,
            current: Some("primary".to_string()),
            fallback_count: 0,
            recovery_count: 0,
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("primary"));
    }

    #[test]
    fn test_cached_response_struct() {
        let cached = CachedResponse {
            key: "test_key".to_string(),
            content: "cached content".to_string(),
            cached_at: 1234567890,
            use_count: 5,
        };

        assert_eq!(cached.key, "test_key");
        assert_eq!(cached.use_count, 5);
    }

    #[test]
    fn test_cached_response_serialize() {
        let cached = CachedResponse {
            key: "k".to_string(),
            content: "c".to_string(),
            cached_at: 0,
            use_count: 0,
        };
        let json = serde_json::to_string(&cached).unwrap();
        let parsed: CachedResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.key, cached.key);
    }

    #[test]
    fn test_offline_stats_default() {
        let stats = OfflineStats::default();
        assert_eq!(stats.cache_hits.load(Ordering::Relaxed), 0);
        assert_eq!(stats.cache_misses.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_offline_handler_new() {
        let handler = OfflineHandler::new(false);
        assert!(!handler.is_enabled());
    }

    #[test]
    fn test_offline_handler_clear_cache() {
        let handler = OfflineHandler::default();
        handler.cache("key1", "value1");
        handler.cache("key2", "value2");

        handler.clear_cache();

        assert!(handler.get_cached("key1").is_none());
        assert!(handler.get_cached("key2").is_none());
    }

    #[test]
    fn test_offline_handler_summary() {
        let handler = OfflineHandler::default();
        handler.cache("key", "value");
        let _ = handler.get_cached("key");
        let _ = handler.get_cached("missing");

        let summary = handler.summary();
        assert!(summary.enabled);
        assert_eq!(summary.cache_size, 1);
        assert_eq!(summary.cache_hits, 1);
        assert_eq!(summary.cache_misses, 1);
    }

    #[test]
    fn test_offline_summary_serialize() {
        let summary = OfflineSummary {
            enabled: true,
            is_offline: false,
            cache_size: 10,
            cache_hits: 50,
            cache_misses: 5,
            times_went_offline: 2,
            times_went_online: 2,
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("cache_hits"));
    }

    #[test]
    fn test_offline_handler_go_offline_twice() {
        let handler = OfflineHandler::default();
        handler.go_offline();
        handler.go_offline(); // Second call shouldn't increment

        let summary = handler.summary();
        assert_eq!(summary.times_went_offline, 1);
    }

    #[test]
    fn test_offline_handler_go_online_when_online() {
        let handler = OfflineHandler::default();
        handler.go_online(); // Already online

        let summary = handler.summary();
        assert_eq!(summary.times_went_online, 0);
    }

    #[test]
    fn test_partial_result_quality() {
        let result: PartialResult<String> =
            PartialResult::partial("data".to_string(), 0.8, vec!["small part".to_string()]);
        assert!(result.quality > 0.5);
        assert!(result.quality < 1.0);
    }

    #[test]
    fn test_partial_result_not_usable() {
        let result: PartialResult<String> =
            PartialResult::partial("data".to_string(), 0.3, vec!["most of it".to_string()]);
        assert!(!result.is_usable);
    }

    #[test]
    fn test_partial_stats_default() {
        let stats = PartialStats::default();
        assert_eq!(stats.complete_results.load(Ordering::Relaxed), 0);
        assert_eq!(stats.partial_results.load(Ordering::Relaxed), 0);
        assert_eq!(stats.failed_results.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_partial_result_manager_default() {
        let manager = PartialResultManager::default();
        let summary = manager.summary();
        assert_eq!(summary.complete_results, 0);
    }

    #[test]
    fn test_partial_result_manager_record() {
        let manager = PartialResultManager::new(0.5);

        let complete: PartialResult<i32> = PartialResult::complete(42);
        manager.record(&complete);

        let partial: PartialResult<i32> = PartialResult::partial(21, 0.6, vec![]);
        manager.record(&partial);

        let failed: PartialResult<i32> = PartialResult::failed(0, "error");
        manager.record(&failed);

        let summary = manager.summary();
        assert_eq!(summary.complete_results, 1);
        assert_eq!(summary.partial_results, 1);
        assert_eq!(summary.failed_results, 1);
    }

    #[test]
    fn test_partial_summary_serialize() {
        let summary = PartialSummary {
            complete_results: 10,
            partial_results: 3,
            failed_results: 1,
            average_completion: 0.85,
            success_rate: 0.93,
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("success_rate"));
    }

    #[test]
    fn test_health_status_variants() {
        assert_eq!(HealthStatus::Healthy, HealthStatus::Healthy);
        assert_eq!(HealthStatus::Degraded, HealthStatus::Degraded);
        assert_eq!(HealthStatus::Unhealthy, HealthStatus::Unhealthy);
        assert_eq!(HealthStatus::Unknown, HealthStatus::Unknown);
    }

    #[test]
    fn test_health_status_serialize() {
        let status = HealthStatus::Degraded;
        let json = serde_json::to_string(&status).unwrap();
        let parsed: HealthStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, status);
    }

    #[test]
    fn test_health_check_struct() {
        let check = HealthCheck {
            service: "api".to_string(),
            status: HealthStatus::Healthy,
            response_time_ms: Some(100),
            last_check: 1234567890,
            error: None,
        };

        assert_eq!(check.service, "api");
        assert_eq!(check.status, HealthStatus::Healthy);
    }

    #[test]
    fn test_health_check_serialize() {
        let check = HealthCheck {
            service: "test".to_string(),
            status: HealthStatus::Unknown,
            response_time_ms: None,
            last_check: 0,
            error: Some("error".to_string()),
        };
        let json = serde_json::to_string(&check).unwrap();
        assert!(json.contains("test"));
    }

    #[test]
    fn test_health_monitor_default() {
        let monitor = HealthMonitor::default();
        assert_eq!(monitor.overall_health(), HealthStatus::Unknown);
    }

    #[test]
    fn test_health_monitor_all_checks() {
        let monitor = HealthMonitor::default();
        monitor.record_check("api1", true, Some(50), None);
        monitor.record_check("api2", true, Some(100), None);

        let checks = monitor.all_checks();
        assert_eq!(checks.len(), 2);
    }

    #[test]
    fn test_health_monitor_overall_health() {
        let monitor = HealthMonitor::default();
        monitor.record_check("api1", true, Some(100), None);
        monitor.record_check("api2", true, Some(100), None);

        assert_eq!(monitor.overall_health(), HealthStatus::Healthy);

        // Add degraded
        monitor.record_check("api3", true, Some(6000), None);
        assert_eq!(monitor.overall_health(), HealthStatus::Degraded);
    }

    #[test]
    fn test_health_monitor_unknown_service() {
        let monitor = HealthMonitor::default();
        assert_eq!(monitor.get_status("nonexistent"), HealthStatus::Unknown);
    }

    #[test]
    fn test_degradation_action_variants() {
        let retry = DegradationAction::Retry;
        let fallback = DegradationAction::SwitchFallback("model".to_string());
        let offline = DegradationAction::GoOffline;
        let fail = DegradationAction::Fail;

        assert_eq!(retry, DegradationAction::Retry);
        assert_eq!(
            fallback,
            DegradationAction::SwitchFallback("model".to_string())
        );
        assert_eq!(offline, DegradationAction::GoOffline);
        assert_eq!(fail, DegradationAction::Fail);
    }

    #[test]
    fn test_degradation_action_clone() {
        let action = DegradationAction::SwitchFallback("test".to_string());
        let cloned = action.clone();
        assert_eq!(cloned, action);
    }

    #[test]
    fn test_degradation_manager_components() {
        let manager = DegradationManager::default();

        // Access all components
        let _ = manager.retry_executor();
        let _ = manager.circuit_breaker();
        let _ = manager.fallback_chain();
        let _ = manager.offline_handler();
        let _ = manager.partial_manager();
        let _ = manager.health_monitor();
    }

    #[test]
    fn test_degradation_manager_fallback_action() {
        let config = DegradationConfig {
            enable_model_fallback: true,
            ..Default::default()
        };
        let manager = DegradationManager::new(config);

        // Add fallback options
        manager.fallback_chain().add_option(FallbackOption {
            id: "primary".to_string(),
            name: "Primary".to_string(),
            priority: 1,
            available: true,
            capability_level: 3,
        });
        manager.fallback_chain().add_option(FallbackOption {
            id: "fallback".to_string(),
            name: "Fallback".to_string(),
            priority: 2,
            available: true,
            capability_level: 2,
        });

        // Set current
        if let Ok(mut current) = manager.fallback_chain().current.write() {
            *current = Some("primary".to_string());
        }

        let failure = RecoverableFailure::new(FailureType::ModelUnavailable, "down");
        let action = manager.handle_failure(&failure);

        assert!(matches!(action, DegradationAction::SwitchFallback(_)));
    }

    #[test]
    fn test_degradation_manager_fail_action() {
        let manager = DegradationManager::default();
        let failure = RecoverableFailure::new(FailureType::ClientError, "bad request");

        let action = manager.handle_failure(&failure);
        assert_eq!(action, DegradationAction::Fail);
    }

    #[test]
    fn test_degradation_summary_serialize() {
        let summary = DegradationSummary {
            retry: RetrySummary {
                total_attempts: 0,
                successful_retries: 0,
                failed_retries: 0,
                total_delay_ms: 0,
            },
            circuit: CircuitSummary {
                state: CircuitState::Closed,
                failure_count: 0,
                times_opened: 0,
                times_closed: 0,
                rejected_requests: 0,
            },
            fallback: FallbackSummary {
                total_options: 0,
                available_options: 0,
                disabled_options: 0,
                current: None,
                fallback_count: 0,
                recovery_count: 0,
            },
            offline: OfflineSummary {
                enabled: true,
                is_offline: false,
                cache_size: 0,
                cache_hits: 0,
                cache_misses: 0,
                times_went_offline: 0,
                times_went_online: 0,
            },
            partial: PartialSummary {
                complete_results: 0,
                partial_results: 0,
                failed_results: 0,
                average_completion: 0.0,
                success_rate: 0.0,
            },
            overall_health: HealthStatus::Healthy,
        };

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("overall_health"));
    }

    #[test]
    fn test_degradation_config_clone() {
        let config = DegradationConfig::default();
        let cloned = config.clone();
        assert_eq!(cloned.max_retries, config.max_retries);
    }

    #[test]
    fn test_retry_strategy_clone() {
        let strategy = RetryStrategy::default();
        let cloned = strategy.clone();
        assert_eq!(cloned.max_attempts, strategy.max_attempts);
    }

    #[test]
    fn test_fallback_option_clone() {
        let option = FallbackOption {
            id: "test".to_string(),
            name: "Test".to_string(),
            priority: 1,
            available: true,
            capability_level: 2,
        };
        let cloned = option.clone();
        assert_eq!(cloned.id, option.id);
    }

    #[test]
    fn test_cached_response_clone() {
        let cached = CachedResponse {
            key: "k".to_string(),
            content: "c".to_string(),
            cached_at: 0,
            use_count: 0,
        };
        let cloned = cached.clone();
        assert_eq!(cloned.key, cached.key);
    }

    #[test]
    fn test_health_check_clone() {
        let check = HealthCheck {
            service: "api".to_string(),
            status: HealthStatus::Healthy,
            response_time_ms: Some(100),
            last_check: 0,
            error: None,
        };
        let cloned = check.clone();
        assert_eq!(cloned.service, check.service);
    }
}
