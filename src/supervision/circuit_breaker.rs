//! Circuit breaker pattern for fault tolerance

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Circuit breaker for protecting against cascading failures
pub struct CircuitBreaker {
    state: AtomicU32, // 0=Closed, 1=Open, 2=HalfOpen
    failure_count: AtomicU32,
    success_count: AtomicU32,
    config: CircuitBreakerConfig,
    last_failure_time: RwLock<Option<Instant>>,
    last_state_change: RwLock<Instant>,
}

/// Circuit breaker configuration
#[derive(Debug, Clone, Copy)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening circuit
    pub failure_threshold: u32,
    /// Number of successes in half-open to close circuit
    pub success_threshold: u32,
    /// Time before attempting half-open
    pub reset_timeout: Duration,
    /// Half-open max requests
    pub half_open_max_requests: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            reset_timeout: Duration::from_secs(30),
            half_open_max_requests: 3,
        }
    }
}

/// Circuit state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,   // Normal operation
    Open,     // Failing, rejecting requests
    HalfOpen, // Testing if service recovered
}

/// Circuit breaker error
#[derive(Debug, Clone)]
pub enum CircuitBreakerError<E> {
    /// Circuit is open
    CircuitOpen,
    /// Operation failed
    OperationFailed(E),
}

impl<E: std::fmt::Display> std::fmt::Display for CircuitBreakerError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CircuitOpen => write!(f, "Circuit breaker is open"),
            Self::OperationFailed(e) => write!(f, "Operation failed: {}", e),
        }
    }
}

impl<E: std::fmt::Debug + std::fmt::Display> std::error::Error for CircuitBreakerError<E> {}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: AtomicU32::new(0),
            failure_count: AtomicU32::new(0),
            success_count: AtomicU32::new(0),
            config,
            last_failure_time: RwLock::new(None),
            last_state_change: RwLock::new(Instant::now()),
        }
    }

    /// Get current circuit state
    pub fn current_state(&self) -> CircuitState {
        match self.state.load(Ordering::Relaxed) {
            0 => CircuitState::Closed,
            1 => CircuitState::Open,
            2 => CircuitState::HalfOpen,
            _ => CircuitState::Closed,
        }
    }

    /// Check if we should attempt reset
    pub async fn should_attempt_reset(&self) -> bool {
        if self.current_state() != CircuitState::Open {
            return false;
        }

        let last_change = *self.last_state_change.read().await;
        last_change.elapsed() >= self.config.reset_timeout
    }

    /// Execute operation with circuit breaker protection
    pub async fn call<F, Fut, T, E>(&self, operation: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
    {
        // Check current state
        match self.current_state() {
            CircuitState::Open => {
                if self.should_attempt_reset().await {
                    self.transition_to(CircuitState::HalfOpen).await;
                } else {
                    warn!("Circuit breaker open, rejecting request");
                    return Err(CircuitBreakerError::CircuitOpen);
                }
            }
            CircuitState::HalfOpen => {
                let requests = self.success_count.load(Ordering::Relaxed)
                    + self.failure_count.load(Ordering::Relaxed);
                if requests >= self.config.half_open_max_requests {
                    warn!("Half-open max requests reached");
                    return Err(CircuitBreakerError::CircuitOpen);
                }
            }
            CircuitState::Closed => {}
        }

        // Execute operation
        match operation().await {
            Ok(result) => {
                self.on_success().await;
                Ok(result)
            }
            Err(e) => {
                self.on_failure().await;
                Err(CircuitBreakerError::OperationFailed(e))
            }
        }
    }

    /// Handle successful operation
    async fn on_success(&self) {
        let success_count = self.success_count.fetch_add(1, Ordering::SeqCst) + 1;
        debug!(success_count = success_count, "Operation succeeded");

        if self.current_state() == CircuitState::HalfOpen {
            if success_count >= self.config.success_threshold {
                info!("Circuit breaker closing after successful recovery");
                self.transition_to(CircuitState::Closed).await;
            }
        } else {
            // Reset failure count in closed state
            self.failure_count.store(0, Ordering::SeqCst);
        }
    }

    /// Handle failed operation
    async fn on_failure(&self) {
        let failure_count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        *self.last_failure_time.write().await = Some(Instant::now());

        warn!(failure_count = failure_count, "Operation failed");

        if self.current_state() == CircuitState::HalfOpen {
            // Any failure in half-open goes back to open
            info!("Failure in half-open, reopening circuit");
            self.transition_to(CircuitState::Open).await;
        } else if failure_count >= self.config.failure_threshold {
            info!("Failure threshold reached, opening circuit");
            self.transition_to(CircuitState::Open).await;
        }
    }

    /// Transition to a new state
    async fn transition_to(&self, new_state: CircuitState) {
        let state_num = match new_state {
            CircuitState::Closed => 0,
            CircuitState::Open => 1,
            CircuitState::HalfOpen => 2,
        };

        let old_state = self.state.swap(state_num, Ordering::SeqCst);
        *self.last_state_change.write().await = Instant::now();

        // Reset counters on state change
        self.failure_count.store(0, Ordering::SeqCst);
        self.success_count.store(0, Ordering::SeqCst);

        info!(
            old_state = ?match old_state {
                0 => CircuitState::Closed,
                1 => CircuitState::Open,
                2 => CircuitState::HalfOpen,
                _ => CircuitState::Closed,
            },
            new_state = ?new_state,
            "Circuit breaker state changed"
        );
    }

    /// Get metrics
    pub fn metrics(&self) -> CircuitBreakerMetrics {
        CircuitBreakerMetrics {
            state: self.current_state(),
            failure_count: self.failure_count.load(Ordering::Relaxed),
            success_count: self.success_count.load(Ordering::Relaxed),
        }
    }
}

/// Circuit breaker metrics
#[derive(Debug, Clone)]
pub struct CircuitBreakerMetrics {
    pub state: CircuitState,
    pub failure_count: u32,
    pub success_count: u32,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn fast_config() -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            reset_timeout: Duration::from_millis(50),
            half_open_max_requests: 2,
        }
    }

    #[test]
    fn test_initial_state_is_closed() {
        let cb = CircuitBreaker::default();
        assert_eq!(cb.current_state(), CircuitState::Closed);
    }

    #[test]
    fn test_default_config_values() {
        let config = CircuitBreakerConfig::default();
        assert_eq!(config.failure_threshold, 5);
        assert_eq!(config.success_threshold, 3);
        assert_eq!(config.reset_timeout, Duration::from_secs(30));
        assert_eq!(config.half_open_max_requests, 3);
    }

    #[test]
    fn test_initial_metrics_are_zero() {
        let cb = CircuitBreaker::default();
        let metrics = cb.metrics();
        assert_eq!(metrics.state, CircuitState::Closed);
        assert_eq!(metrics.failure_count, 0);
        assert_eq!(metrics.success_count, 0);
    }

    #[tokio::test]
    async fn test_success_keeps_circuit_closed() {
        let cb = CircuitBreaker::new(fast_config());

        let result: Result<i32, CircuitBreakerError<String>> =
            cb.call(|| async { Ok(42) }).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(cb.current_state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_failures_below_threshold_stay_closed() {
        let cb = CircuitBreaker::new(fast_config());

        // Cause 2 failures (threshold is 3)
        for _ in 0..2 {
            let _: Result<i32, _> =
                cb.call(|| async { Err::<i32, String>("fail".into()) }).await;
        }

        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert_eq!(cb.metrics().failure_count, 2);
    }

    #[tokio::test]
    async fn test_transition_to_open_after_failure_threshold() {
        let cb = CircuitBreaker::new(fast_config());

        // Cause 3 failures (threshold is 3)
        for _ in 0..3 {
            let _: Result<i32, _> =
                cb.call(|| async { Err::<i32, String>("fail".into()) }).await;
        }

        assert_eq!(cb.current_state(), CircuitState::Open);
    }

    #[tokio::test]
    async fn test_open_circuit_rejects_requests() {
        let cb = CircuitBreaker::new(fast_config());

        // Trip the breaker
        for _ in 0..3 {
            let _: Result<i32, _> =
                cb.call(|| async { Err::<i32, String>("fail".into()) }).await;
        }
        assert_eq!(cb.current_state(), CircuitState::Open);

        // Next call should be rejected immediately
        let result: Result<i32, CircuitBreakerError<String>> =
            cb.call(|| async { Ok(42) }).await;

        assert!(matches!(result, Err(CircuitBreakerError::CircuitOpen)));
    }

    #[tokio::test]
    async fn test_half_open_after_reset_timeout() {
        let cb = CircuitBreaker::new(fast_config());

        // Trip the breaker
        for _ in 0..3 {
            let _: Result<i32, _> =
                cb.call(|| async { Err::<i32, String>("fail".into()) }).await;
        }
        assert_eq!(cb.current_state(), CircuitState::Open);

        // Wait for the reset timeout
        tokio::time::sleep(Duration::from_millis(60)).await;

        // should_attempt_reset should be true
        assert!(cb.should_attempt_reset().await);

        // Next call should transition to half-open and succeed
        let result: Result<i32, CircuitBreakerError<String>> =
            cb.call(|| async { Ok(1) }).await;
        assert!(result.is_ok());
        assert_eq!(cb.current_state(), CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn test_half_open_to_closed_after_success_threshold() {
        let cb = CircuitBreaker::new(fast_config());

        // Trip the breaker
        for _ in 0..3 {
            let _: Result<i32, _> =
                cb.call(|| async { Err::<i32, String>("fail".into()) }).await;
        }

        // Wait for reset timeout
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Succeed enough times (success_threshold = 2)
        for _ in 0..2 {
            let result: Result<i32, CircuitBreakerError<String>> =
                cb.call(|| async { Ok(1) }).await;
            assert!(result.is_ok());
        }

        assert_eq!(cb.current_state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_half_open_failure_reopens_circuit() {
        let cb = CircuitBreaker::new(fast_config());

        // Trip the breaker
        for _ in 0..3 {
            let _: Result<i32, _> =
                cb.call(|| async { Err::<i32, String>("fail".into()) }).await;
        }

        // Wait for reset timeout
        tokio::time::sleep(Duration::from_millis(60)).await;

        // One success to get into half-open
        let _: Result<i32, CircuitBreakerError<String>> =
            cb.call(|| async { Ok(1) }).await;
        assert_eq!(cb.current_state(), CircuitState::HalfOpen);

        // Fail in half-open -> back to open
        let _: Result<i32, _> =
            cb.call(|| async { Err::<i32, String>("fail again".into()) }).await;
        assert_eq!(cb.current_state(), CircuitState::Open);
    }

    #[tokio::test]
    async fn test_should_attempt_reset_false_when_closed() {
        let cb = CircuitBreaker::default();
        assert!(!cb.should_attempt_reset().await);
    }

    #[tokio::test]
    async fn test_should_attempt_reset_false_before_timeout() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            reset_timeout: Duration::from_secs(60),
            ..CircuitBreakerConfig::default()
        };
        let cb = CircuitBreaker::new(config);

        // Trip breaker
        let _: Result<i32, _> =
            cb.call(|| async { Err::<i32, String>("fail".into()) }).await;
        assert_eq!(cb.current_state(), CircuitState::Open);

        // Should not reset yet (timeout is 60s)
        assert!(!cb.should_attempt_reset().await);
    }

    #[tokio::test]
    async fn test_success_resets_failure_count_in_closed() {
        let cb = CircuitBreaker::new(fast_config());

        // Cause 2 failures (below threshold of 3)
        for _ in 0..2 {
            let _: Result<i32, _> =
                cb.call(|| async { Err::<i32, String>("fail".into()) }).await;
        }
        assert_eq!(cb.metrics().failure_count, 2);

        // A success should reset failure count
        let _: Result<i32, CircuitBreakerError<String>> =
            cb.call(|| async { Ok(1) }).await;
        assert_eq!(cb.metrics().failure_count, 0);
    }

    #[tokio::test]
    async fn test_metrics_track_successes() {
        let cb = CircuitBreaker::new(fast_config());

        for _ in 0..4 {
            let _: Result<i32, CircuitBreakerError<String>> =
                cb.call(|| async { Ok(1) }).await;
        }

        let metrics = cb.metrics();
        assert_eq!(metrics.state, CircuitState::Closed);
        // success_count resets to 0 on each success in closed state because
        // failure_count is reset, but success_count still increments
        assert_eq!(metrics.success_count, 4);
    }

    #[test]
    fn test_circuit_breaker_error_display() {
        let open_err: CircuitBreakerError<String> = CircuitBreakerError::CircuitOpen;
        assert_eq!(format!("{}", open_err), "Circuit breaker is open");

        let op_err: CircuitBreakerError<String> =
            CircuitBreakerError::OperationFailed("db timeout".into());
        assert_eq!(format!("{}", op_err), "Operation failed: db timeout");
    }
}
