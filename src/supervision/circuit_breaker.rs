//! Circuit breaker pattern for fault tolerance

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
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
    Closed,    // Normal operation
    Open,      // Failing, rejecting requests
    HalfOpen,  // Testing if service recovered
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
    pub async fn call<F, Fut, T, E>(
        &self,
        operation: F,
    ) -> Result<T, CircuitBreakerError<E>>
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
                let requests = self.success_count.load(Ordering::Relaxed) + 
                              self.failure_count.load(Ordering::Relaxed);
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
