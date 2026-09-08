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

// ---------------------------------------------------------------------------
// Wall-budget task boundary (review finding #6): the anchor is latched once
// per ApiClient; the agent resets it at each run_task so a multi-task
// session does not fail every request after the first task's budget elapses.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reset_wall_budget_relatches_a_fresh_window() {
    let client = wall_budget_client(Some(1));
    expire_wall_budget(&client, 1).await;
    assert!(
        client.wall_budget_stop().is_some(),
        "budget must report exhausted before the reset"
    );

    // Task boundary: run_task resets the client anchor alongside the agent's
    // own per-task clock. The next billable request must latch a NEW window
    // instead of failing on the previous task's exhausted budget.
    client.reset_wall_budget();
    assert!(
        client.wall_budget_stop().is_none(),
        "a fresh window must open after the reset"
    );
    let deadline = client.run_wall_deadline().expect("deadline relatched");
    assert!(
        deadline > Instant::now(),
        "relatched deadline must lie in the future"
    );
}

// ---------------------------------------------------------------------------
// Bounded error-body reads (review finding #11): stream_client has no total
// reqwest timeout, so error-status bodies are read with a time bound and a
// byte cap. A proxy that sends 429/5xx headers then stalls must terminate
// with the typed status, not hang a headless run forever.
// ---------------------------------------------------------------------------

/// Local copy of the api::tests helper: drain the request headers before
/// responding (writing without reading resets the connection on Windows).
async fn drain_request(socket: &mut tokio::net::TcpStream) {
    use tokio::io::AsyncReadExt;
    let mut buf = [0u8; 4096];
    let mut total = Vec::new();
    loop {
        let n = socket.read(&mut buf).await.unwrap_or(0);
        if n == 0 {
            break;
        }
        total.extend_from_slice(&buf[..n]);
        // End of HTTP headers is marked by \r\n\r\n. The request body (a
        // small JSON payload) may follow in the same or a later read — the
        // client waits for our response either way, so headers suffice.
        if total.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
}

#[tokio::test]
async fn stalled_error_body_terminates_with_typed_status() {
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        drain_request(&mut socket).await;
        // 429 headers, then stall forever with the connection open — the
        // error body never arrives.
        socket
            .write_all(
                b"HTTP/1.1 429 Too Many Requests\r\nContent-Type: text/plain\r\nTransfer-Encoding: chunked\r\n\r\n",
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_secs(30)).await;
    });

    let mut config = crate::config::Config {
        endpoint: format!("http://127.0.0.1:{}/v1", addr.port()),
        ..Default::default()
    };
    config.retry.max_retries = 0; // fail fast: a single attempt
    let client = ApiClient::new(&config).unwrap();

    let started = Instant::now();
    let err = client
        .chat_stream(vec![Message::user("hi")], None, ThinkingMode::Disabled)
        .await
        .expect_err("a stalled error body must not hang the run");
    assert!(
        started.elapsed() < Duration::from_secs(25),
        "the error-body read must be time-bounded, took {:?}",
        started.elapsed()
    );
    let status = err
        .chain()
        .find_map(|c| c.downcast_ref::<ApiError>())
        .and_then(|e| match e {
            ApiError::HttpStatus { status, .. } => Some(*status),
            _ => None,
        });
    assert_eq!(status, Some(429), "typed 429 outcome, got: {err:?}");

    let _ = server.await;
}

#[tokio::test]
async fn endless_error_body_is_capped_and_typed() {
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        drain_request(&mut socket).await;
        socket
            .write_all(
                b"HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/plain\r\nTransfer-Encoding: chunked\r\n\r\n",
            )
            .await
            .unwrap();
        // Stream body bytes forever; the client must stop at the byte cap
        // (which also closes the connection and ends this loop).
        let chunk = format!("{:X}\r\n{}\r\n", 1024, "x".repeat(1024));
        loop {
            if socket.write_all(chunk.as_bytes()).await.is_err() {
                break;
            }
        }
    });

    let mut config = crate::config::Config {
        endpoint: format!("http://127.0.0.1:{}/v1", addr.port()),
        ..Default::default()
    };
    config.retry.max_retries = 0;
    let client = ApiClient::new(&config).unwrap();

    let started = Instant::now();
    let err = client
        .chat(vec![Message::user("hi")], None, ThinkingMode::Disabled)
        .await
        .expect_err("an endless error body must not be buffered forever");
    assert!(
        started.elapsed() < Duration::from_secs(25),
        "the byte cap must bound the read, took {:?}",
        started.elapsed()
    );
    let status = err
        .chain()
        .find_map(|c| c.downcast_ref::<ApiError>())
        .and_then(|e| match e {
            ApiError::HttpStatus { status, .. } => Some(*status),
            _ => None,
        });
    assert_eq!(status, Some(500), "typed 500 outcome, got: {err:?}");

    let _ = server.await;
}

/// A server `Retry-After` that lands past the wall deadline must not launch
/// one more billable request after the backoff sleep (review finding #11).
#[tokio::test]
async fn retry_after_past_wall_deadline_never_posts_again() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let hits = std::sync::Arc::new(AtomicUsize::new(0));
    let hits_server = std::sync::Arc::clone(&hits);

    let server = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            hits_server.fetch_add(1, Ordering::SeqCst);
            drain_request(&mut socket).await;
            let body = "rate limited";
            let response = format!(
                "HTTP/1.1 429 Too Many Requests\r\nContent-Type: text/plain\r\nRetry-After: 3\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            if socket.write_all(response.as_bytes()).await.is_err() {
                break;
            }
        }
    });

    let mut config = crate::config::Config {
        endpoint: format!("http://127.0.0.1:{}/v1", addr.port()),
        ..Default::default()
    };
    config.retry.max_retries = 3;
    config.retry.base_delay_ms = 100;
    config.retry.max_delay_ms = 10_000;
    config.agent.max_wall_secs = Some(1); // expires during the 3s Retry-After
    let client = ApiClient::new(&config).unwrap();

    let err = client
        .chat(vec![Message::user("hi")], None, ThinkingMode::Disabled)
        .await
        .expect_err("the run must stop as a budget stop, not retry forever");
    assert!(
        err.chain()
            .any(|c| c.downcast_ref::<WallClockBudgetExceeded>().is_some()),
        "expected WallClockBudgetExceeded after the expired retry wait, got: {err:?}"
    );
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "no expired retry may launch a second billable request"
    );

    server.abort();
}

// ---------------------------------------------------------------------------
// Profile knob wiring (review finding #12): when the client's main endpoint
// matches the resolved [models.default] profile, its max_retries and
// response_timeout_floor_secs override the global defaults on BOTH the
// streaming and non-streaming paths.
// ---------------------------------------------------------------------------

/// Client whose `[models.default]` profile points at the same endpoint as
/// the top-level config (the loader-synthesized shape), with the given
/// per-profile knobs.
fn profiled_client(
    endpoint: &str,
    profile_max_retries: Option<u32>,
    profile_floor: Option<u64>,
) -> ApiClient {
    let mut config = crate::config::Config {
        endpoint: endpoint.to_string(),
        ..Default::default()
    };
    config.models.insert(
        "default".to_string(),
        crate::config::ModelProfile {
            endpoint: endpoint.to_string(),
            model: config.model.clone(),
            api_key: None,
            max_tokens: config.max_tokens,
            temperature: config.temperature,
            modalities: vec!["text".to_string()],
            context_length: config.context_length,
            extra_body: None,
            native_function_calling: None,
            max_retries: profile_max_retries,
            response_timeout_floor_secs: profile_floor,
        },
    );
    ApiClient::new(&config).unwrap()
}

#[test]
fn active_profile_requires_matching_endpoint() {
    let client = profiled_client("http://127.0.0.1:9/v1", Some(0), None);
    assert!(
        client.active_profile().is_some(),
        "same-endpoint default profile must be active"
    );

    // A profile for a DIFFERENT endpoint must not leak its knobs onto this
    // client (e.g. after a recovery endpoint switch).
    let mut config = crate::config::Config {
        endpoint: "http://127.0.0.1:9/v1".to_string(),
        ..Default::default()
    };
    config.models.insert(
        "default".to_string(),
        crate::config::ModelProfile {
            endpoint: "http://127.0.0.1:9999/v1".to_string(),
            model: config.model.clone(),
            api_key: None,
            max_tokens: config.max_tokens,
            temperature: config.temperature,
            modalities: vec!["text".to_string()],
            context_length: config.context_length,
            extra_body: None,
            native_function_calling: None,
            max_retries: Some(0),
            response_timeout_floor_secs: None,
        },
    );
    let client = ApiClient::new(&config).unwrap();
    assert!(
        client.active_profile().is_none(),
        "mismatched-endpoint profile must stay inactive"
    );
}

#[test]
fn profile_floor_raises_stream_header_timeout() {
    let client = profiled_client("http://127.0.0.1:9/v1", None, Some(1800));
    assert_eq!(client.stream_header_timeout_secs(), 1800);

    let no_floor = profiled_client("http://127.0.0.1:9/v1", None, None);
    assert_eq!(
        no_floor.stream_header_timeout_secs(),
        no_floor.config.agent.step_timeout_secs.max(120),
        "without a floor the legacy max(step_timeout, 120) applies"
    );
}

#[test]
fn profile_floor_raises_nonstreaming_response_timeout() {
    let client = profiled_client("http://127.0.0.1:9/v1", None, Some(7200));
    assert!(
        client.response_timeout_secs_for(client.active_profile()) >= 7200,
        "profile floor must raise the adaptive non-streaming timeout"
    );
}

/// Regression: a profile with `max_retries = 0` fails fast on BOTH paths —
/// one billable attempt each, even when the global retry budget is larger.
#[tokio::test]
async fn profile_max_retries_zero_fails_fast_on_both_paths() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint = format!("http://127.0.0.1:{}/v1", addr.port());
    let hits = std::sync::Arc::new(AtomicUsize::new(0));
    let hits_server = std::sync::Arc::clone(&hits);

    let server = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            hits_server.fetch_add(1, Ordering::SeqCst);
            drain_request(&mut socket).await;
            let body = "server error";
            let response = format!(
                "HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            if socket.write_all(response.as_bytes()).await.is_err() {
                break;
            }
        }
    });

    let mut config = crate::config::Config {
        endpoint: endpoint.clone(),
        ..Default::default()
    };
    // Global budget would allow 6 attempts; the profile must override it.
    config.retry.max_retries = 5;
    config.retry.base_delay_ms = 1;
    config.retry.max_delay_ms = 5;
    config.models.insert(
        "default".to_string(),
        crate::config::ModelProfile {
            endpoint: endpoint.clone(),
            model: config.model.clone(),
            api_key: None,
            max_tokens: config.max_tokens,
            temperature: config.temperature,
            modalities: vec!["text".to_string()],
            context_length: config.context_length,
            extra_body: None,
            native_function_calling: None,
            max_retries: Some(0),
            response_timeout_floor_secs: None,
        },
    );
    let client = ApiClient::new(&config).unwrap();

    let _ = client
        .chat(vec![Message::user("hi")], None, ThinkingMode::Disabled)
        .await;
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "non-streaming: profile max_retries=0 must fail fast"
    );

    let _ = client
        .chat_stream(vec![Message::user("hi")], None, ThinkingMode::Disabled)
        .await;
    assert_eq!(
        hits.load(Ordering::SeqCst),
        2,
        "streaming: profile max_retries=0 must fail fast"
    );

    // Control: without the profile override the global retry budget applies
    // (1 initial attempt + 2 retries = 3 more hits).
    let mut plain_config = crate::config::Config {
        endpoint,
        ..Default::default()
    };
    plain_config.retry.max_retries = 2;
    plain_config.retry.base_delay_ms = 1;
    plain_config.retry.max_delay_ms = 5;
    let plain = ApiClient::new(&plain_config).unwrap();
    let _ = plain
        .chat(vec![Message::user("hi")], None, ThinkingMode::Disabled)
        .await;
    assert_eq!(
        hits.load(Ordering::SeqCst),
        2 + 3,
        "control: global max_retries=2 must still retry when no profile overrides it"
    );

    server.abort();
}
