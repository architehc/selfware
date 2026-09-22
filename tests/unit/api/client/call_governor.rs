//! Regression tests for the per-call cost governor (2026-09-22 long-task e2e
//! finding: "cost has no governor — single calls exceeded 5 minutes, one
//! 7-minute call returned zero content tokens, and nothing capped or reported
//! per-call time").
//!
//! Covers the three fix parts:
//!   1. per-call wall-time measurement: every LLM call records elapsed_ms,
//!      emitted on `LlmResponseReceived` and folded into the run's cumulative
//!      `CallLatencyStats` (count / total / max / slowest-call detail);
//!   2. `agent.max_call_secs`: when set, a call exceeding it fails typed
//!      (`CallTimeBudgetExceeded`), terminal, never a silent hang — and the
//!      default stays uncapped;
//!   3. a completed call that burned a long wait and returned zero content +
//!      no tool calls fails typed as `ApiError::ZeroContentLongCall` with the
//!      elapsed time named.

use super::*;
use crate::agent::progress::{ProgressEvent, RecordingProgressEmitter};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const ANSWER: &str = r#"{"id":"x","object":"chat.completion","created":0,"model":"m","choices":[{"index":0,"message":{"role":"assistant","content":"done"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":10,"total_tokens":15,"cost":0.02}}"#;
/// A completed response with empty content, no tool calls, no reasoning, and
/// zero completion tokens — the observed 7-minute-call shape, miniaturized.
const EMPTY: &str = r#"{"id":"x","object":"chat.completion","created":0,"model":"m","choices":[{"index":0,"message":{"role":"assistant","content":""},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":0,"total_tokens":5}}"#;
const SSE: &str = "data: {\"choices\":[{\"delta\":{\"content\":\"answer\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":10,\"total_tokens\":15}}\n\ndata: [DONE]\n\n";

fn config(endpoint: String) -> crate::config::Config {
    let mut config = crate::config::Config {
        endpoint,
        ..Default::default()
    };
    config.retry.max_retries = 0;
    config.agent.native_function_calling = true;
    config
}

/// Read one full request (headers + Content-Length body) so the client is
/// never reset by a premature response write.
async fn read_request(socket: &mut tokio::net::TcpStream) {
    let mut request = Vec::new();
    loop {
        let mut chunk = [0; 4096];
        let count = socket.read(&mut chunk).await.unwrap();
        assert!(count > 0);
        request.extend_from_slice(&chunk[..count]);
        if let Some(split) = request.windows(4).position(|w| w == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&request[..split]);
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    line.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse().unwrap())
                })
                .unwrap();
            if request.len() >= split + 4 + length {
                break;
            }
        }
    }
}

/// Serve each (delay, body) pair in order: read the request, wait `delay`,
/// then respond 200. A client that already gave up (the cap test) makes the
/// write fail — that is the point of the fixture, not a fixture failure.
async fn scripted_server(
    responses: Vec<(Duration, &'static str)>,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        for (delay, body) in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            read_request(&mut socket).await;
            tokio::time::sleep(delay).await;
            let wire = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(wire.as_bytes()).await;
        }
    });
    (endpoint, task)
}

#[tokio::test]
async fn slow_call_records_elapsed_in_event_and_run_stats() {
    let (endpoint, task) = scripted_server(vec![(Duration::from_millis(400), ANSWER)]).await;
    let mut client = ApiClient::new(&config(endpoint)).unwrap();
    let recorder = Arc::new(RecordingProgressEmitter::new());
    client.with_progress_emitter(recorder.clone());

    // Uncapped by default: the slow call must SUCCEED (no max_call_secs in
    // the default config) — and still be measured.
    assert!(client.config().agent.max_call_secs.is_none());
    client
        .chat(vec![Message::user("answer")], None, ThinkingMode::Enabled)
        .await
        .unwrap();

    // Per-call reporting: the response event carries the measured wall time.
    let elapsed = recorder.snapshot().iter().find_map(|e| match e {
        ProgressEvent::LlmResponseReceived { elapsed_ms, .. } => Some(*elapsed_ms),
        _ => None,
    });
    let elapsed = elapsed.expect("LlmResponseReceived must be emitted");
    assert!(
        elapsed >= 300,
        "measured {elapsed}ms for a response delayed 400ms"
    );

    // Cumulative stats: count / total / max / slowest-call detail.
    let stats = client.call_latency_stats();
    assert_eq!(stats.call_count, 1);
    assert_eq!(stats.max_ms, stats.total_ms);
    assert!(stats.total_ms >= 300, "stats saw {}ms", stats.total_ms);
    let slowest = stats.slowest.expect("slowest call detail recorded");
    assert_eq!(slowest.path, "chat");
    assert!(!slowest.model.is_empty());
    task.await.unwrap();
}

#[tokio::test]
async fn per_call_cap_aborts_hanging_call_with_typed_error() {
    // Server holds the response 3s; the 1s cap must abort long before that.
    let (endpoint, task) = scripted_server(vec![(Duration::from_secs(3), ANSWER)]).await;
    let mut config = config(endpoint);
    config.agent.max_call_secs = Some(1);
    let client = ApiClient::new(&config).unwrap();

    let started = std::time::Instant::now();
    let err = client
        .chat(vec![Message::user("answer")], None, ThinkingMode::Enabled)
        .await
        .unwrap_err();
    let wall = started.elapsed();

    let cap = err
        .downcast_ref::<CallTimeBudgetExceeded>()
        .unwrap_or_else(|| panic!("expected typed CallTimeBudgetExceeded, got: {err:#}"));
    assert_eq!(cap.limit_secs, 1);
    assert!(cap.elapsed_secs >= 1);
    assert!(
        wall < Duration::from_secs(3),
        "cap must abort the call, not wait out the server ({wall:?})"
    );
    assert!(
        !counts_toward_circuit_breaker(&err),
        "a deliberate cap stop is not a sick backend"
    );
    // The aborted call burned wall time and must still be measured.
    let stats = client.call_latency_stats();
    assert_eq!(stats.call_count, 1);
    assert!(stats.total_ms >= 900, "stats saw {}ms", stats.total_ms);
    task.await.unwrap();
}

#[tokio::test]
async fn zero_content_long_call_is_typed_and_names_elapsed() {
    let (endpoint, task) = scripted_server(vec![(Duration::from_millis(300), EMPTY)]).await;
    let mut client = ApiClient::new(&config(endpoint)).unwrap();
    client.zero_content_threshold_override_ms = Some(50);
    let recorder = Arc::new(RecordingProgressEmitter::new());
    client.with_progress_emitter(recorder.clone());

    let err = client
        .chat(vec![Message::user("answer")], None, ThinkingMode::Enabled)
        .await
        .unwrap_err();

    match err.downcast_ref::<crate::errors::ApiError>() {
        Some(crate::errors::ApiError::ZeroContentLongCall { elapsed_ms }) => {
            assert!(*elapsed_ms >= 200, "elapsed time named: {elapsed_ms}ms");
        }
        other => panic!("expected ZeroContentLongCall, got {other:?}"),
    }
    // The call still reported itself: the per-call event fired (before the
    // typed failure) and the run stats counted it.
    assert!(recorder
        .snapshot()
        .iter()
        .any(|e| matches!(e, ProgressEvent::LlmResponseReceived { .. })));
    assert_eq!(client.call_latency_stats().call_count, 1);
    task.await.unwrap();
}

#[tokio::test]
async fn fast_empty_response_is_not_the_long_call_outcome() {
    // Same empty shape, but fast: below the threshold it passes through as a
    // plain (Ok, empty) response exactly as before — the typed outcome is
    // about burned wall time, not emptiness alone.
    let (endpoint, task) = scripted_server(vec![(Duration::ZERO, EMPTY)]).await;
    let mut client = ApiClient::new(&config(endpoint)).unwrap();
    client.zero_content_threshold_override_ms = Some(50);
    let resp = client
        .chat(vec![Message::user("answer")], None, ThinkingMode::Enabled)
        .await
        .unwrap();
    assert!(resp.choices[0].message.content.text().is_empty());
    task.await.unwrap();
}

#[tokio::test]
async fn streaming_collect_records_full_call_wall_time() {
    let (endpoint, task) = scripted_server(vec![(Duration::from_millis(250), SSE)]).await;
    let client = ApiClient::new(&config(endpoint)).unwrap();
    let resp = client
        .chat_stream(vec![Message::user("answer")], None, ThinkingMode::Enabled)
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();
    assert_eq!(resp.choices[0].message.content.text(), "answer");

    let stats = client.call_latency_stats();
    assert_eq!(stats.call_count, 1);
    let slowest = stats.slowest.expect("stream call recorded");
    assert_eq!(slowest.path, "chat_stream");
    assert!(
        stats.max_ms >= 200,
        "stream task must record send→end wall time, got {}ms",
        stats.max_ms
    );
    task.await.unwrap();
}

#[tokio::test]
async fn stats_accumulate_across_calls_and_name_the_slowest() {
    let (endpoint, task) = scripted_server(vec![
        (Duration::from_millis(100), ANSWER),
        (Duration::from_millis(300), SSE),
    ])
    .await;
    let client = ApiClient::new(&config(endpoint)).unwrap();
    client
        .chat(vec![Message::user("answer")], None, ThinkingMode::Enabled)
        .await
        .unwrap();
    client
        .chat_stream(vec![Message::user("answer")], None, ThinkingMode::Enabled)
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();

    let stats = client.call_latency_stats();
    assert_eq!(stats.call_count, 2);
    assert!(
        stats.total_ms >= 350,
        "both calls folded in, got {}ms",
        stats.total_ms
    );
    let slowest = stats.slowest.unwrap();
    assert_eq!(slowest.path, "chat_stream");
    assert_eq!(slowest.elapsed_ms, stats.max_ms);
    task.await.unwrap();
}
