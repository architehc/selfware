use super::*;
use crate::api::types::Message;
use crate::config::{Config, ExecutionMode};
use crate::testing::mock_api::{MockLlmServer, MockResponse};

fn http_status_err(status: u16) -> anyhow::Error {
    crate::errors::ApiError::HttpStatus {
        status,
        message: format!("body for {status}"),
    }
    .into()
}

#[test]
fn terminal_4xx_statuses_are_classified_terminal() {
    for status in [400u16, 401, 403, 404, 422] {
        let err = http_status_err(status);
        assert!(
            is_terminal_api_client_error(&err),
            "status {status} must be classified terminal"
        );
    }
}

#[test]
fn retryable_statuses_are_not_classified_terminal() {
    // 429 and 5xx stay retryable: the planning retry loop and the
    // streaming→non-streaming fallback must still apply to them.
    for status in [429u16, 500, 502, 503, 504] {
        let err = http_status_err(status);
        assert!(
            !is_terminal_api_client_error(&err),
            "status {status} must stay retryable"
        );
    }
}

#[test]
fn terminal_status_is_found_through_anyhow_context() {
    // Errors bubble up wrapped in anyhow context (e.g. "planning failed");
    // the classifier must walk the whole chain, not just the top level.
    let err = http_status_err(401).context("Streaming failed");
    assert!(is_terminal_api_client_error(&err));
}

#[test]
fn non_http_status_errors_are_not_classified_terminal() {
    let network: anyhow::Error =
        crate::errors::ApiError::Network("connection reset".to_string()).into();
    assert!(!is_terminal_api_client_error(&network));

    let timeout: anyhow::Error = crate::errors::ApiError::Timeout.into();
    assert!(!is_terminal_api_client_error(&timeout));

    let plain = anyhow::anyhow!("some other failure");
    assert!(!is_terminal_api_client_error(&plain));
}

#[test]
fn wall_clock_budget_exceeded_is_classified_terminal() {
    // The run-level wall budget is already exhausted: a planning-level
    // retry would only burn backoff sleeps (the client blocks the actual
    // billable request), so the stop must be terminal — and stay
    // classified as a budget stop rather than a transient network error.
    let err: anyhow::Error = crate::api::client::WallClockBudgetExceeded {
        elapsed_secs: 51,
        limit_secs: 8,
    }
    .into();
    assert!(is_terminal_api_client_error(&err));

    // Through anyhow context wrapping too.
    let wrapped = err.context("planning failed");
    assert!(is_terminal_api_client_error(&wrapped));
}

// -----------------------------------------------------------------------
// Streaming → non-streaming fallback behavior (mock server)
// -----------------------------------------------------------------------

fn mock_agent_config(endpoint: String, streaming: bool) -> Config {
    Config {
        endpoint,
        model: "mock-model".to_string(),
        context_length: 500_000,
        max_tokens: 8192,
        agent: crate::config::AgentConfig {
            max_iterations: 8,
            step_timeout_secs: 30,
            stream_stall_timeout_secs: None,
            streaming,
            native_function_calling: false,
            min_completion_steps: 0,
            require_verification_before_completion: false,
            ..Default::default()
        },
        safety: crate::config::SafetyConfig {
            allowed_paths: vec!["./**".to_string(), "/**".to_string()],
            ..Default::default()
        },
        execution_mode: ExecutionMode::Yolo,
        ..Default::default()
    }
}

/// A terminal 401 from the streaming attempt must NOT trigger the
/// non-streaming fallback: the same request fails identically and the
/// duplicate hit only delays the remediation hint. Client-level retries
/// are disabled so each API attempt is exactly one HTTP request.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn streaming_fallback_is_skipped_on_terminal_401() {
    let server = MockLlmServer::builder()
        .with_default_response(MockResponse::Error {
            status: 401,
            body: r#"{"error":"No cookie auth credentials found"}"#.to_string(),
        })
        .build()
        .await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), true);
    config.retry = crate::config::RetrySettings {
        max_retries: 0,
        base_delay_ms: 1,
        max_delay_ms: 1,
    };
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("Hello"));

    let result = agent.get_assistant_step_response(false).await;

    let err = result
        .err()
        .expect("terminal 401 must propagate as an error")
        .to_string();
    assert!(
        err.contains("Hint"),
        "expected the auth remediation hint to surface, got: {err}"
    );
    let requests = server.captured_request_bodies().await;
    assert_eq!(
        requests.len(),
        1,
        "non-streaming fallback must not re-hit a terminal 401, got {} requests",
        requests.len()
    );
    server.stop().await;
}

/// Positive control: a retryable streaming failure (500) must still fall
/// back to the non-streaming endpoint, which can then succeed.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn streaming_fallback_still_applies_to_retryable_failures() {
    let server = MockLlmServer::builder()
        .with_error(500, r#"{"error":"internal server error"}"#)
        .with_response("fallback answer")
        .build()
        .await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), true);
    config.retry = crate::config::RetrySettings {
        max_retries: 0,
        base_delay_ms: 1,
        max_delay_ms: 1,
    };
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("Hello"));

    let result = agent.get_assistant_step_response(false).await;

    assert!(
        result.is_ok(),
        "fallback should succeed on retryable failures: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap().content, "fallback answer");
    let requests = server.captured_request_bodies().await;
    assert_eq!(
        requests.len(),
        2,
        "expected one streaming attempt plus one fallback request, got {}",
        requests.len()
    );
    server.stop().await;
}

// -----------------------------------------------------------------------
// Non-streaming usage accumulation (session token totals)
// -----------------------------------------------------------------------

/// Non-streaming configs must feed the session-wide token counters too:
/// previously the totals stayed 0 for the whole run because only the
/// streaming path's SSE usage arm recorded them.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn non_streaming_step_accumulates_session_token_totals() {
    let server = MockLlmServer::builder()
        .with_response("sync answer")
        .build()
        .await;
    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("Hello"));

    let (before_prompt, before_completion) = crate::output::get_total_tokens();
    let result = agent.get_assistant_step_response(false).await;
    assert!(
        result.is_ok(),
        "non-streaming step should succeed: {:?}",
        result.err()
    );

    // The mock reports usage of 10 prompt / 5 completion per response.
    // Concurrent tests can only ADD to these process-global counters,
    // never subtract, so a >= assertion on the delta is race-safe.
    let (after_prompt, after_completion) = crate::output::get_total_tokens();
    assert!(
            after_prompt.saturating_sub(before_prompt) >= 10,
            "non-streaming usage must accumulate prompt tokens (before={before_prompt}, after={after_prompt})"
        );
    assert!(
            after_completion.saturating_sub(before_completion) >= 5,
            "non-streaming usage must accumulate completion tokens (before={before_completion}, after={after_completion})"
        );
    server.stop().await;
}

/// The streaming→non-streaming fallback is also a non-streaming response:
/// its usage must land in the session totals as well.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn streaming_fallback_accumulates_session_token_totals() {
    let server = MockLlmServer::builder()
        .with_error(500, r#"{"error":"internal server error"}"#)
        .with_response("fallback answer")
        .build()
        .await;
    let mut config = mock_agent_config(format!("{}/v1", server.url()), true);
    config.retry = crate::config::RetrySettings {
        max_retries: 0,
        base_delay_ms: 1,
        max_delay_ms: 1,
    };
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("Hello"));

    let (before_prompt, before_completion) = crate::output::get_total_tokens();
    let result = agent.get_assistant_step_response(false).await;
    assert!(
        result.is_ok(),
        "fallback step should succeed: {:?}",
        result.err()
    );

    let (after_prompt, after_completion) = crate::output::get_total_tokens();
    assert!(
            after_prompt.saturating_sub(before_prompt) >= 10,
            "fallback usage must accumulate prompt tokens (before={before_prompt}, after={after_prompt})"
        );
    assert!(
            after_completion.saturating_sub(before_completion) >= 5,
            "fallback usage must accumulate completion tokens (before={before_completion}, after={after_completion})"
        );
    server.stop().await;
}
