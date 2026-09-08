use super::*;

#[test]
fn detect_backend_returns_string_for_unresponsive() {
    // A port that is almost certainly closed.
    let result = detect_backend("http://127.0.0.1:59999/v1");
    // Should either fail (network error) or return "unknown".
    if let Ok(label) = result {
        assert!(
            ["llama.cpp", "sglang", "vllm", "unknown"].contains(&label.as_str()),
            "unexpected backend label: {}",
            label
        );
    }
    // network error is acceptable
}

fn wall_budget_client(max_wall_secs: Option<u64>) -> ApiClient {
    let mut config = crate::config::Config {
        endpoint: "http://127.0.0.1:9/v1".to_string(), // discard port: never listens
        ..Default::default()
    };
    config.agent.max_wall_secs = max_wall_secs;
    ApiClient::new(&config).unwrap()
}

#[test]
fn wall_budget_stop_is_none_without_budget_or_within_budget() {
    // No budget configured: never stops, no anchor latched.
    let client = wall_budget_client(None);
    assert!(client.wall_budget_stop().is_none());
    assert!(client.run_wall_deadline().is_none());

    // Budget configured, run just started: within budget.
    let client = wall_budget_client(Some(600));
    assert!(client.wall_budget_stop().is_none());
}

#[test]
fn run_wall_deadline_is_latched_once_and_shared_across_clones() {
    let client = wall_budget_client(Some(600));
    let d1 = client.run_wall_deadline().expect("deadline");
    std::thread::sleep(Duration::from_millis(20));
    // A second call must NOT slide the window forward...
    let d2 = client.run_wall_deadline().expect("deadline");
    assert_eq!(d1, d2, "run deadline must be latched, not refreshed");
    // ...and clones of the client share the same anchor.
    let clone = client.clone();
    assert_eq!(Some(d1), clone.run_wall_deadline());
}

/// Force the run anchor far enough into the past that the budget is
/// already exhausted. Falls back to a real (short) sleep on platforms
/// whose monotonic clock cannot go back 120s.
async fn expire_wall_budget(client: &ApiClient, limit_secs: u64) {
    let anchor = Instant::now()
        .checked_sub(Duration::from_secs(limit_secs + 120))
        .unwrap_or_else(|| {
            // Clock cannot go back: latch "now" and the test sleeps past
            // the (1s) deadline below.
            Instant::now()
        });
    *client
        .wall_budget_start
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(anchor);
    if anchor.elapsed().as_secs() <= limit_secs {
        tokio::time::sleep(Duration::from_secs(limit_secs + 1)).await;
    }
}

#[tokio::test]
async fn wall_budget_stop_classified_as_budget_not_network() {
    let client = wall_budget_client(Some(1));
    expire_wall_budget(&client, 1).await;

    let stop = client
        .wall_budget_stop()
        .expect("budget must be reported as exhausted");
    let err = stop.to_string();
    assert!(
        err.contains("Wall-clock timeout"),
        "budget stop must carry the canonical reason, got: {}",
        err
    );
    assert!(
        stop.downcast_ref::<WallClockBudgetExceeded>().is_some(),
        "stop must be a WallClockBudgetExceeded"
    );
}

#[tokio::test]
async fn no_billable_request_is_issued_after_wall_budget_expiry() {
    let client = wall_budget_client(Some(1));
    expire_wall_budget(&client, 1).await;

    // Non-streaming path: must fail fast with the budget stop instead of
    // attempting (and retrying) a connection to the dead endpoint.
    let started = Instant::now();
    let err = client
        .chat(Vec::new(), None, ThinkingMode::Disabled)
        .await
        .expect_err("chat must fail once the wall budget is exhausted");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "budget stop must not burn retry backoff: {:?}",
        started.elapsed()
    );
    assert!(
        err.chain()
            .any(|c| c.downcast_ref::<WallClockBudgetExceeded>().is_some()),
        "expected WallClockBudgetExceeded, got: {:?}",
        err
    );

    // Streaming path: same guarantee.
    let err = client
        .chat_stream(Vec::new(), None, ThinkingMode::Disabled)
        .await
        .expect_err("chat_stream must fail once the wall budget is exhausted");
    assert!(
        err.chain()
            .any(|c| c.downcast_ref::<WallClockBudgetExceeded>().is_some()),
        "expected WallClockBudgetExceeded, got: {:?}",
        err
    );
}

#[test]
fn wall_budget_stop_message_matches_canonical_budget_reason() {
    // Same wording as Agent::enforce_hard_budgets so the failure-mode
    // classifier files the stop as a wall-budget stop, not a network error.
    let err: anyhow::Error = WallClockBudgetExceeded {
        elapsed_secs: 51,
        limit_secs: 8,
    }
    .into();
    assert_eq!(err.to_string(), "Wall-clock timeout: 51s >= 8s");
}

#[test]
fn circuit_breaker_classifier_counts_only_transient_errors() {
    let network: anyhow::Error = ApiError::Network("connection reset".into()).into();
    assert!(counts_toward_circuit_breaker(&network));

    let timeout: anyhow::Error = ApiError::Timeout.into();
    assert!(counts_toward_circuit_breaker(&timeout));

    let rate_limited: anyhow::Error = ApiError::RateLimit {
        retry_after_secs: Some(1),
    }
    .into();
    assert!(counts_toward_circuit_breaker(&rate_limited));

    let server_error: anyhow::Error = ApiError::HttpStatus {
        status: 503,
        message: "unavailable".into(),
    }
    .into();
    assert!(counts_toward_circuit_breaker(&server_error));

    let too_many: anyhow::Error = ApiError::HttpStatus {
        status: 429,
        message: "slow down".into(),
    }
    .into();
    assert!(counts_toward_circuit_breaker(&too_many));

    let auth: anyhow::Error = ApiError::HttpStatus {
        status: 401,
        message: "bad key".into(),
    }
    .into();
    assert!(!counts_toward_circuit_breaker(&auth));

    let bad_request: anyhow::Error = ApiError::HttpStatus {
        status: 400,
        message: "invalid".into(),
    }
    .into();
    assert!(!counts_toward_circuit_breaker(&bad_request));

    let overflow: anyhow::Error = ApiError::ContextOverflow("too long".into()).into();
    assert!(!counts_toward_circuit_breaker(&overflow));

    let parse: anyhow::Error = ApiError::Parse("not json".into()).into();
    assert!(!counts_toward_circuit_breaker(&parse));

    // The run-level wall-clock budget stop is a deliberate halt, not a sick
    // backend — it must not trip the breaker either.
    let budget: anyhow::Error = WallClockBudgetExceeded {
        elapsed_secs: 10,
        limit_secs: 8,
    }
    .into();
    assert!(!counts_toward_circuit_breaker(&budget));
}

// ---------------------------------------------------------------------------
// http_status_error redaction (P1): upstream gateways can echo the API key
// in error bodies; the key must not reach headless output via the error.
// ---------------------------------------------------------------------------

#[test]
fn http_status_error_redacts_configured_api_key_echoed_in_body() {
    let key = crate::config::RedactedString::new("sk-test-1234567890");
    let err = ApiClient::http_status_error(
        "https://api.example.com/v1",
        reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        "upstream failure: invalid key sk-test-1234567890 provided".to_string(),
        Some(&key),
    );
    let msg = err.to_string();
    assert!(
        !msg.contains("sk-test-1234567890"),
        "configured key must be redacted from the error, got: {msg}"
    );
    assert!(
        msg.contains("[REDACTED]"),
        "redaction marker present: {msg}"
    );
    assert!(
        msg.contains("upstream failure"),
        "non-secret body content preserved: {msg}"
    );
}

#[test]
fn http_status_error_redacts_key_in_401_hint_path() {
    let key = crate::config::RedactedString::new("sk-test-1234567890");
    let err = ApiClient::http_status_error(
        "https://api.example.com/v1",
        reqwest::StatusCode::UNAUTHORIZED,
        "401 No cookie auth credentials found for sk-test-1234567890".to_string(),
        Some(&key),
    );
    let msg = err.to_string();
    assert!(
        !msg.contains("sk-test-1234567890"),
        "configured key must be redacted from the 401 error, got: {msg}"
    );
    assert!(
        msg.contains("SELFWARE_API_KEY"),
        "remediation hint preserved: {msg}"
    );
}

#[test]
fn http_status_error_ignores_short_or_absent_key() {
    // A short key (< 8 chars) is not literal-replaced (too collision-prone);
    // generic secret-pattern redaction still runs.
    let short = crate::config::RedactedString::new("abc");
    let err = ApiClient::http_status_error(
        "https://api.example.com/v1",
        reqwest::StatusCode::BAD_REQUEST,
        "bad request: abc".to_string(),
        Some(&short),
    );
    assert!(err.to_string().contains("bad request"));
    let err = ApiClient::http_status_error(
        "https://api.example.com/v1",
        reqwest::StatusCode::BAD_REQUEST,
        "bad request".to_string(),
        None,
    );
    assert!(err.to_string().contains("bad request"));
}
