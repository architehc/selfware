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

#[tokio::test]
async fn test_classifier_permanent_failures_do_not_count() {
    let cb = CircuitBreaker::new(fast_config());

    // More failures than the threshold (3), but the classifier rejects them:
    // the error must pass through and the breaker must stay closed.
    for _ in 0..5 {
        let result: Result<i32, CircuitBreakerError<String>> = cb
            .call_with_classifier(
                || async { Err::<i32, String>("permanent".into()) },
                |_| false,
            )
            .await;
        assert!(matches!(
            result,
            Err(CircuitBreakerError::OperationFailed(_))
        ));
    }

    assert_eq!(cb.current_state(), CircuitState::Closed);
    assert_eq!(cb.metrics().failure_count, 0);
}

#[tokio::test]
async fn test_classifier_transient_failures_open_circuit() {
    let cb = CircuitBreaker::new(fast_config());

    for _ in 0..3 {
        let _: Result<i32, _> = cb
            .call_with_classifier(
                || async { Err::<i32, String>("transient".into()) },
                |_| true,
            )
            .await;
    }

    assert_eq!(cb.current_state(), CircuitState::Open);
}

#[tokio::test]
async fn test_half_open_cancelled_future_releases_slot_and_late_open_success_ignored() {
    let cb = CircuitBreaker::new(CircuitBreakerConfig {
        failure_threshold: 2,
        success_threshold: 2,
        reset_timeout: Duration::from_millis(40),
        half_open_max_requests: 1,
    });

    // Trip the breaker
    for _ in 0..2 {
        let _: Result<i32, _> = cb
            .call(|| async { Err::<i32, String>("fail".into()) })
            .await;
    }
    assert_eq!(cb.current_state(), CircuitState::Open);

    // A late success while Open must not pre-populate success_count for HalfOpen
    cb.on_success().await;
    assert_eq!(cb.metrics().success_count, 0);

    // Wait for reset timeout so next call enters HalfOpen
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Start a slow HalfOpen call and cancel (drop) it mid-flight
    let timed_out = tokio::time::timeout(
        Duration::from_millis(15),
        cb.call(|| async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            Ok::<i32, String>(1)
        }),
    )
    .await;
    assert!(timed_out.is_err(), "call should time out and drop future");
    assert_eq!(cb.current_state(), CircuitState::HalfOpen);

    // Because HalfOpenSlotGuard released the slot on drop, a subsequent call must succeed
    let first: Result<i32, CircuitBreakerError<String>> = cb.call(|| async { Ok(1) }).await;
    assert!(first.is_ok());
    // Still HalfOpen after 1 real success (late Open success was ignored, threshold is 2)
    assert_eq!(cb.current_state(), CircuitState::HalfOpen);

    let second: Result<i32, CircuitBreakerError<String>> = cb.call(|| async { Ok(2) }).await;
    assert!(second.is_ok());
    assert_eq!(cb.current_state(), CircuitState::Closed);
}

#[tokio::test]
async fn rust_circuit_breaker_matches_lean_formal_table() {
    let raw = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/formal/supervision_table.json"
    ));
    let rows: Vec<[String; 3]> =
        serde_json::from_str(raw).expect("valid formal/supervision_table.json");

    let mut checked = 0usize;
    for [domain, outcome_name, expected] in &rows {
        let Some(state_label) = domain.strip_prefix("cb:") else {
            continue;
        };

        let reset_timeout = if state_label == "open_reset_ready" {
            Duration::ZERO
        } else {
            Duration::from_secs(3600)
        };
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            reset_timeout,
            half_open_max_requests: 1,
        });

        match state_label {
            "closed_fresh" => {}
            "closed_one_fail" => {
                cb.failure_count.store(1, Ordering::SeqCst);
            }
            "open_waiting" | "open_reset_ready" => {
                cb.state.store(1, Ordering::SeqCst);
            }
            "half_open_probe_0" => {
                cb.state.store(2, Ordering::SeqCst);
            }
            "half_open_probe_1" => {
                cb.state.store(2, Ordering::SeqCst);
                cb.success_count.store(1, Ordering::SeqCst);
            }
            "half_open_saturated" => {
                cb.state.store(2, Ordering::SeqCst);
                cb.half_open_requests.store(1, Ordering::SeqCst);
            }
            other => panic!("unknown cb state label in formal table: {other}"),
        }

        let res: Result<i32, CircuitBreakerError<String>> = match outcome_name.as_str() {
            "counted_success" => cb.call_with_classifier(|| async { Ok(1) }, |_| true).await,
            "counted_failure" => {
                cb.call_with_classifier(|| async { Err("counted".to_string()) }, |_| true)
                    .await
            }
            "uncounted_failure" => {
                cb.call_with_classifier(|| async { Err("uncounted".to_string()) }, |_| false)
                    .await
            }
            other => panic!("unknown cb outcome in formal table: {other}"),
        };

        let res_name = match res {
            Ok(_) => "ok",
            Err(CircuitBreakerError::OperationFailed(_)) => "failed_op",
            Err(CircuitBreakerError::CircuitOpen) => "circuit_open",
        };
        let state_name = match cb.current_state() {
            CircuitState::Closed => "closed",
            CircuitState::Open => "open",
            CircuitState::HalfOpen => "half_open",
        };
        let fc = cb.failure_count.load(Ordering::SeqCst);
        let sc = cb.success_count.load(Ordering::SeqCst);
        let ho = cb.half_open_requests.load(Ordering::SeqCst);
        let actual = format!("{res_name}:{state_name}:f{fc}:s{sc}:h{ho}");

        assert_eq!(
            &actual, expected,
            "Lean SV1 CircuitBreaker mismatch for ({domain}, {outcome_name})"
        );
        checked += 1;
    }

    assert_eq!(
        checked, 21,
        "all 21 SV1 CircuitBreaker rows in formal/supervision_table.json must be verified"
    );
}
