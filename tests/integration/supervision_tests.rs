use selfware::supervision::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitState,
};
use std::time::Duration;

#[tokio::test]
async fn test_circuit_breaker_transitions() {
    let config = CircuitBreakerConfig {
        failure_threshold: 3,
        success_threshold: 2,
        reset_timeout: Duration::from_millis(100),
        half_open_max_requests: 2,
    };

    let cb = CircuitBreaker::new(config);
    assert_eq!(cb.current_state(), CircuitState::Closed);

    // Fail 3 times to trip circuit
    for _ in 0..3 {
        let result: Result<(), CircuitBreakerError<String>> =
            cb.call(|| async { Err("Failed".to_string()) }).await;
        assert!(matches!(
            result,
            Err(CircuitBreakerError::OperationFailed(_))
        ));
    }

    // Circuit should now be open
    assert_eq!(cb.current_state(), CircuitState::Open);

    // Next call should immediately fail with CircuitOpen
    let result: Result<(), CircuitBreakerError<String>> = cb.call(|| async { Ok(()) }).await;
    assert!(matches!(result, Err(CircuitBreakerError::CircuitOpen)));

    // Wait for reset timeout
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Should transition to half-open on next call
    let result: Result<(), CircuitBreakerError<String>> = cb.call(|| async { Ok(()) }).await;
    assert!(result.is_ok());
    assert_eq!(cb.current_state(), CircuitState::HalfOpen);

    // One more success should close the circuit
    let result: Result<(), CircuitBreakerError<String>> = cb.call(|| async { Ok(()) }).await;
    assert!(result.is_ok());
    assert_eq!(cb.current_state(), CircuitState::Closed);
}

#[tokio::test]
async fn test_circuit_breaker_half_open_failure() {
    let config = CircuitBreakerConfig {
        failure_threshold: 1,
        success_threshold: 2,
        reset_timeout: Duration::from_millis(100),
        half_open_max_requests: 2,
    };

    let cb = CircuitBreaker::new(config);

    // Trip circuit
    let _: Result<(), CircuitBreakerError<String>> =
        cb.call(|| async { Err("Failed".to_string()) }).await;
    assert_eq!(cb.current_state(), CircuitState::Open);

    tokio::time::sleep(Duration::from_millis(150)).await;

    // Half-open failure should trip circuit back to Open immediately
    let _: Result<(), CircuitBreakerError<String>> =
        cb.call(|| async { Err("Failed again".to_string()) }).await;
    assert_eq!(cb.current_state(), CircuitState::Open);
}
