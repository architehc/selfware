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

    let result: Result<i32, CircuitBreakerError<String>> = cb.call(|| async { Ok(42) }).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
    assert_eq!(cb.current_state(), CircuitState::Closed);
}

#[tokio::test]
async fn test_failures_below_threshold_stay_closed() {
    let cb = CircuitBreaker::new(fast_config());

    // Cause 2 failures (threshold is 3)
    for _ in 0..2 {
        let _: Result<i32, _> = cb
            .call(|| async { Err::<i32, String>("fail".into()) })
            .await;
    }

    assert_eq!(cb.current_state(), CircuitState::Closed);
    assert_eq!(cb.metrics().failure_count, 2);
}

#[tokio::test]
async fn test_transition_to_open_after_failure_threshold() {
    let cb = CircuitBreaker::new(fast_config());

    // Cause 3 failures (threshold is 3)
    for _ in 0..3 {
        let _: Result<i32, _> = cb
            .call(|| async { Err::<i32, String>("fail".into()) })
            .await;
    }

    assert_eq!(cb.current_state(), CircuitState::Open);
}

#[tokio::test]
async fn test_open_circuit_rejects_requests() {
    let cb = CircuitBreaker::new(fast_config());

    // Trip the breaker
    for _ in 0..3 {
        let _: Result<i32, _> = cb
            .call(|| async { Err::<i32, String>("fail".into()) })
            .await;
    }
    assert_eq!(cb.current_state(), CircuitState::Open);

    // Next call should be rejected immediately
    let result: Result<i32, CircuitBreakerError<String>> = cb.call(|| async { Ok(42) }).await;

    assert!(matches!(result, Err(CircuitBreakerError::CircuitOpen)));
}

#[tokio::test]
async fn test_half_open_after_reset_timeout() {
    let cb = CircuitBreaker::new(fast_config());

    // Trip the breaker
    for _ in 0..3 {
        let _: Result<i32, _> = cb
            .call(|| async { Err::<i32, String>("fail".into()) })
            .await;
    }
    assert_eq!(cb.current_state(), CircuitState::Open);

    // Wait for the reset timeout
    tokio::time::sleep(Duration::from_millis(60)).await;

    // should_attempt_reset should be true
    assert!(cb.should_attempt_reset().await);

    // Next call should transition to half-open and succeed
    let result: Result<i32, CircuitBreakerError<String>> = cb.call(|| async { Ok(1) }).await;
    assert!(result.is_ok());
    assert_eq!(cb.current_state(), CircuitState::HalfOpen);
}

#[tokio::test]
async fn test_half_open_to_closed_after_success_threshold() {
    let cb = CircuitBreaker::new(fast_config());

    // Trip the breaker
    for _ in 0..3 {
        let _: Result<i32, _> = cb
            .call(|| async { Err::<i32, String>("fail".into()) })
            .await;
    }

    // Wait for reset timeout
    tokio::time::sleep(Duration::from_millis(60)).await;

    // Succeed enough times (success_threshold = 2)
    for _ in 0..2 {
        let result: Result<i32, CircuitBreakerError<String>> = cb.call(|| async { Ok(1) }).await;
        assert!(result.is_ok());
    }

    assert_eq!(cb.current_state(), CircuitState::Closed);
}

#[tokio::test]
async fn test_half_open_failure_reopens_circuit() {
    let cb = CircuitBreaker::new(fast_config());

    // Trip the breaker
    for _ in 0..3 {
        let _: Result<i32, _> = cb
            .call(|| async { Err::<i32, String>("fail".into()) })
            .await;
    }

    // Wait for reset timeout
    tokio::time::sleep(Duration::from_millis(60)).await;

    // One success to get into half-open
    let _: Result<i32, CircuitBreakerError<String>> = cb.call(|| async { Ok(1) }).await;
    assert_eq!(cb.current_state(), CircuitState::HalfOpen);

    // Fail in half-open -> back to open
    let _: Result<i32, _> = cb
        .call(|| async { Err::<i32, String>("fail again".into()) })
        .await;
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
    let _: Result<i32, _> = cb
        .call(|| async { Err::<i32, String>("fail".into()) })
        .await;
    assert_eq!(cb.current_state(), CircuitState::Open);

    // Should not reset yet (timeout is 60s)
    assert!(!cb.should_attempt_reset().await);
}

#[tokio::test]
async fn test_success_resets_failure_count_in_closed() {
    let cb = CircuitBreaker::new(fast_config());

    // Cause 2 failures (below threshold of 3)
    for _ in 0..2 {
        let _: Result<i32, _> = cb
            .call(|| async { Err::<i32, String>("fail".into()) })
            .await;
    }
    assert_eq!(cb.metrics().failure_count, 2);

    // A success should reset failure count
    let _: Result<i32, CircuitBreakerError<String>> = cb.call(|| async { Ok(1) }).await;
    assert_eq!(cb.metrics().failure_count, 0);
}

#[tokio::test]
async fn test_metrics_track_successes() {
    let cb = CircuitBreaker::new(fast_config());

    for _ in 0..4 {
        let _: Result<i32, CircuitBreakerError<String>> = cb.call(|| async { Ok(1) }).await;
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
