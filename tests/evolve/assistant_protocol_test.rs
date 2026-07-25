//! Review protocol: typed outcomes and the one budgeted repair.
//!
//! A scripted mock chat endpoint (same raw-TCP pattern as
//! `envelope_evidence_test.rs`) serves per-request canned completions so the
//! `GroundedAssistant::review` path can be driven through malformed-then-valid,
//! twice-malformed, empty, and fully-ungrounded model output.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use axum::http::StatusCode;

use selfware::config::Config;
use selfware::evolve::assistant::{evidence_from_document, GroundedAssistant, ReviewProtocolError};
use selfware::evolve::{EvolveServer, Graph, Node};

use crate::{get_json, post_json};

const REPAIR_TEXT: &str =
    "Your previous reply was not valid JSON matching the required schema. Respond with ONLY the JSON object.";

/// One grounded claim citing E1 — the fixture document produces exactly E1.
const VALID_REVIEW: &str =
    r#"{"claims":[{"text":"grounded","evidence_ids":["E1"]}],"recommendations":[]}"#;

/// Review where every item cites unknown evidence — fully ungrounded.
const UNGROUNDED_REVIEW: &str = r#"{
    "claims": [
        {"text": "Unknown citation", "evidence_ids": ["E404"]},
        {"text": "No citation", "evidence_ids": []}
    ],
    "recommendations": [{
        "title": "Ungrounded",
        "rationale": "No known source",
        "evidence_ids": ["E404"],
        "hops": []
    }]
}"#;

struct MockState {
    requests: AtomicUsize,
    bodies: Mutex<Vec<String>>,
}

/// Mock OpenAI-compatible endpoint serving `contents` per request (the last
/// entry repeats when the script is exhausted). Returns the join handle, the
/// endpoint base URL, and the shared observation state.
async fn spawn_scripted_mock(
    contents: Vec<String>,
) -> (tokio::task::JoinHandle<()>, String, Arc<MockState>) {
    let state = Arc::new(MockState {
        requests: AtomicUsize::new(0),
        bodies: Mutex::new(Vec::new()),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_state = state.clone();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let contents = contents.clone();
            let state = server_state.clone();
            tokio::spawn(async move {
                // Drain the request (headers + content-length body) before
                // responding so large evidence payloads cannot break the pipe.
                let mut received = Vec::new();
                let mut buf = [0u8; 8192];
                let mut expected: Option<usize> = None;
                let mut body_start = 0usize;
                loop {
                    let Ok(n) = stream.read(&mut buf).await else {
                        return;
                    };
                    if n == 0 {
                        return;
                    }
                    received.extend_from_slice(&buf[..n]);
                    if expected.is_none() {
                        if let Some(headers_end) =
                            received.windows(4).position(|window| window == b"\r\n\r\n")
                        {
                            let headers =
                                String::from_utf8_lossy(&received[..headers_end]).to_lowercase();
                            let length: usize = headers
                                .lines()
                                .find_map(|line| line.strip_prefix("content-length:"))
                                .and_then(|value| value.trim().parse().ok())
                                .unwrap_or(0);
                            body_start = headers_end + 4;
                            expected = Some(body_start + length);
                        }
                    }
                    if expected.is_some_and(|total| received.len() >= total) {
                        break;
                    }
                }
                state
                    .bodies
                    .lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&received[body_start..]).to_string());
                let index = state.requests.fetch_add(1, Ordering::SeqCst);
                let content = &contents[index.min(contents.len() - 1)];
                let body = json!({
                    "id": "chatcmpl-test",
                    "object": "chat.completion",
                    "created": 0,
                    "model": "mock-review",
                    "choices": [{
                        "index": 0,
                        "message": { "role": "assistant", "content": content },
                        "finish_reason": "stop"
                    }],
                    "usage": { "prompt_tokens": 11, "completion_tokens": 7, "total_tokens": 18 }
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });

    (
        handle,
        format!("http://127.0.0.1:{}/v1", addr.port()),
        state,
    )
}

fn assistant(endpoint: &str) -> GroundedAssistant {
    let config = Config {
        endpoint: endpoint.to_string(),
        ..Config::default()
    };
    GroundedAssistant::new(&config).unwrap()
}

fn fixture_evidence() -> Vec<selfware::evolve::assistant::GroundingEvidence> {
    evidence_from_document("src/lib.rs", "pub fn grounded() {}\n", "hash", 40).0
}

#[tokio::test]
async fn malformed_once_then_valid_is_repaired_and_succeeds() {
    let (_mock, endpoint, state) = spawn_scripted_mock(vec![
        "this is not json".to_string(),
        VALID_REVIEW.to_string(),
    ])
    .await;

    let review = assistant(&endpoint)
        .review("Review this file", fixture_evidence(), true)
        .await
        .unwrap();

    assert_eq!(review.model, "mock-review");
    assert_eq!(review.claims.len(), 1);
    assert_eq!(state.requests.load(Ordering::SeqCst), 2);
    let bodies = state.bodies.lock().unwrap();
    let repair_request: Value = serde_json::from_str(&bodies[1]).unwrap();
    let repair_messages = repair_request["messages"].as_array().unwrap();
    assert!(
        repair_messages.iter().any(|message| message["content"]
            .as_str()
            .unwrap_or_default()
            .contains(REPAIR_TEXT)),
        "the second request must carry the repair instruction: {repair_request}"
    );
    assert!(
        repair_messages.iter().any(|message| message["content"]
            .as_str()
            .unwrap_or_default()
            .contains("Review this file")),
        "the repair request must keep the original user prompt as context"
    );
}

#[tokio::test]
async fn malformed_twice_yields_invalid_protocol_error_with_telemetry() {
    let (_mock, endpoint, state) =
        spawn_scripted_mock(vec!["nope".to_string(), "still nope".to_string()]).await;

    let err = assistant(&endpoint)
        .review("Review this file", fixture_evidence(), true)
        .await
        .unwrap_err();
    let protocol = err
        .downcast_ref::<ReviewProtocolError>()
        .expect("review failure must be a typed ReviewProtocolError");

    assert!(matches!(protocol, ReviewProtocolError::Invalid { .. }));
    let body = protocol.body();
    assert_eq!(body["error"], "model_output_invalid");
    assert!(body["detail"].is_string());
    assert_eq!(body["model"], "mock-review");
    assert!(body["latency_ms"].is_number());
    assert_eq!(body["usage"]["total_tokens"], 18);
    assert_eq!(state.requests.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn empty_review_yields_empty_protocol_error() {
    let (_mock, endpoint, state) =
        spawn_scripted_mock(vec![r#"{"claims": [], "recommendations": []}"#.to_string()]).await;

    let err = assistant(&endpoint)
        .review("Review this file", fixture_evidence(), true)
        .await
        .unwrap_err();
    let protocol = err
        .downcast_ref::<ReviewProtocolError>()
        .expect("empty review must be a typed ReviewProtocolError");

    assert!(matches!(protocol, ReviewProtocolError::Empty { .. }));
    let body = protocol.body();
    assert_eq!(body["error"], "model_output_empty");
    assert_eq!(body["model"], "mock-review");
    assert!(body["latency_ms"].is_number());
    assert_eq!(body["usage"]["total_tokens"], 18);
    // Valid JSON on the first attempt: no repair call.
    assert_eq!(state.requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn fully_ungrounded_review_yields_ungrounded_protocol_error() {
    let (_mock, endpoint, state) = spawn_scripted_mock(vec![UNGROUNDED_REVIEW.to_string()]).await;

    let err = assistant(&endpoint)
        .review("Review this file", fixture_evidence(), true)
        .await
        .unwrap_err();
    let protocol = err
        .downcast_ref::<ReviewProtocolError>()
        .expect("ungrounded review must be a typed ReviewProtocolError");

    match protocol {
        ReviewProtocolError::Ungrounded { rejected_items, .. } => {
            assert!(*rejected_items > 0)
        }
        other => panic!("expected Ungrounded, got {other:?}"),
    }
    let body = protocol.body();
    assert_eq!(body["error"], "model_output_ungrounded");
    assert!(body["rejected_items"].as_u64().unwrap() > 0);
    assert_eq!(body["model"], "mock-review");
    assert!(body["latency_ms"].is_number());
    assert_eq!(body["usage"]["total_tokens"], 18);
    assert_eq!(state.requests.load(Ordering::SeqCst), 1);
}

// --- HTTP mapping: trust_state on 200s, typed 422 bodies (spec §2.1, §3.1) ---

/// Server pinned against a single-file project, with the model endpoint
/// pointing at a scripted mock. Selected-document scope is used throughout:
/// it requires no graph membership and keeps evidence deterministic.
async fn review_server(endpoint: &str, content: &str) -> (tempfile::TempDir, EvolveServer) {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("reviewed.rs"), content).unwrap();
    let graph = Graph {
        nodes: vec![Node::code("crate::reviewed", "src/reviewed.rs")],
        edges: vec![],
    };
    let mut config = Config::default();
    config.endpoint = endpoint.to_string();
    config.context_length = 1_000_000;
    let server = EvolveServer::with_config(graph, dir.path(), &config).unwrap();
    (dir, server)
}

/// POST /api/assistant/review (selected-document scope) against the fixture.
async fn post_review(server: &EvolveServer) -> (StatusCode, Value) {
    let (status, document) = get_json(server, "/api/ide/document?path=src/reviewed.rs").await;
    assert_eq!(status, StatusCode::OK);
    let expected_hash = document["hash"].as_str().unwrap().to_string();
    let (status, body) = post_json(
        server,
        "/api/assistant/review",
        json!({
            "path": "src/reviewed.rs",
            "question": "Review this file",
            "expected_hash": expected_hash,
            "scope": "selected_document"
        }),
    )
    .await;
    (status, serde_json::from_str(&body).unwrap())
}

#[tokio::test]
async fn clean_review_reports_structural_trust_state() {
    let (_mock, endpoint, _state) = spawn_scripted_mock(vec![VALID_REVIEW.to_string()]).await;
    let (_dir, server) = review_server(&endpoint, "pub fn grounded() {}\n").await;

    let (status, body) = post_review(&server).await;

    assert_eq!(status, StatusCode::OK, "review failed: {body}");
    let review = &body["review"];
    assert_eq!(review["trust_state"], "structural");
    assert_eq!(review["citation_valid"], true);
    assert_eq!(review["evidence_complete"], true);
    assert_eq!(review["semantic_validation"], "not_performed");
}

#[tokio::test]
async fn partial_evidence_review_reports_degraded_trust_state() {
    // 900 lines against the 800-line selected-document budget truncates the
    // snapshot (the partial-evidence fixture pattern from assistant_test.rs),
    // so the review succeeds but evidence_complete is false.
    let content = (1..=900)
        .map(|line| format!("pub fn item_{line}() {{}}\n"))
        .collect::<String>();
    let (_mock, endpoint, _state) = spawn_scripted_mock(vec![VALID_REVIEW.to_string()]).await;
    let (_dir, server) = review_server(&endpoint, &content).await;

    let (status, body) = post_review(&server).await;

    assert_eq!(status, StatusCode::OK, "review failed: {body}");
    let review = &body["review"];
    assert_eq!(review["evidence_complete"], false);
    assert_eq!(review["trust_state"], "degraded");
}

#[tokio::test]
async fn malformed_twice_maps_to_422_with_spec_body() {
    let (_mock, endpoint, state) =
        spawn_scripted_mock(vec!["nope".to_string(), "still nope".to_string()]).await;
    let (_dir, server) = review_server(&endpoint, "pub fn grounded() {}\n").await;

    let (status, body) = post_review(&server).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "body: {body}");
    assert_eq!(body["error"], "model_output_invalid");
    assert!(body["detail"].is_string());
    assert_eq!(body["model"], "mock-review");
    assert!(body["latency_ms"].is_number());
    assert_eq!(body["usage"]["total_tokens"], 18);
    // One budgeted repair, then the typed failure.
    assert_eq!(state.requests.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn fully_ungrounded_maps_to_422_with_rejected_items() {
    let (_mock, endpoint, _state) = spawn_scripted_mock(vec![UNGROUNDED_REVIEW.to_string()]).await;
    let (_dir, server) = review_server(&endpoint, "pub fn grounded() {}\n").await;

    let (status, body) = post_review(&server).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "body: {body}");
    assert_eq!(body["error"], "model_output_ungrounded");
    assert!(
        body["rejected_items"].as_u64().unwrap() >= 1,
        "ungrounded body must report rejected items: {body}"
    );
    assert_eq!(body["model"], "mock-review");
    assert!(body["latency_ms"].is_number());
    assert_eq!(body["usage"]["total_tokens"], 18);
}
