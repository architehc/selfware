//! Envelope-backed evidence: the assistant evidence paths ship tier-projected
//! context from the cached ContextEnvelope, and the preview/outbound responses
//! share its `content_hash`.

use axum::http::StatusCode;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use selfware::config::Config;
use selfware::evolve::{EvolveServer, Graph, Node};

use crate::{edge, get_json, post_json};

/// The reviewed file: small, with one distinctive body line that must survive
/// verbatim in every tier (the selected document is never projected).
const REVIEWED_RS: &str = r#"//! Reviewed module docs.

/// Computes the answer.
pub fn answer() -> usize {
    let marker = 41 + 1;
    marker
}
"#;

/// A neighbor with real body weight and comments, so the lite skeleton is
/// dramatically smaller than the verbatim source.
const NEIGHBOR_RS: &str = r#"//! Neighbor module docs, padding the comment overhead.

/// Adds one.
pub fn add_one(x: usize) -> usize {
    // inner comment
    let mut acc = x + 1;
    // accumulate over a small range so the body has real weight
    for i in 0..x {
        acc += i * 2;
        acc %= 1024;
    }
    acc
}

/// Adds two with a match-heavy body.
pub fn add_two(x: usize) -> usize {
    // classify the input, then build a label string
    let label = match x {
        0 => "empty",
        1..=4 => "short",
        _ => "long",
    };
    let mut out = String::from(label);
    out.push_str("done");
    x + 2 + out.len()
}

/// Multiplies into a running accumulator.
pub fn mul_acc(x: usize, y: usize) -> usize {
    // running product with a modular cap
    let mut acc = 1usize;
    for i in 0..y {
        acc *= x + i;
        acc %= 4096;
    }
    acc
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_adds() {
        assert_eq!(super::add_one(1), 2);
    }
}
"#;

/// Temp project with both files on disk and a graph that links them, so the
/// neighbor rides the active-context evidence as a priority document.
fn fixture() -> (tempfile::TempDir, Graph) {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("reviewed.rs"), REVIEWED_RS).unwrap();
    std::fs::write(src.join("neighbor.rs"), NEIGHBOR_RS).unwrap();
    let mut reviewed = Node::code("crate::reviewed", "src/reviewed.rs");
    reviewed.tokens = selfware::token_count::estimate_content_tokens(REVIEWED_RS);
    let mut neighbor = Node::code("crate::neighbor", "src/neighbor.rs");
    neighbor.tokens = selfware::token_count::estimate_content_tokens(NEIGHBOR_RS);
    (
        dir,
        Graph {
            nodes: vec![reviewed, neighbor],
            edges: vec![edge("crate::reviewed", "crate::neighbor")],
        },
    )
}

/// Mock OpenAI-compatible endpoint returning one canned, grounded review.
async fn spawn_mock_review_server() -> (tokio::task::JoinHandle<()>, String) {
    let body = json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "created": 0,
        "model": "mock-review",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "{\"claims\":[{\"text\":\"grounded\",\"evidence_ids\":[\"E1\"]}],\"recommendations\":[]}"
            },
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
    })
    .to_string();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let body = body.clone();
            tokio::spawn(async move {
                // Drain the request (headers + content-length body) before
                // responding so large evidence payloads cannot break the pipe.
                let mut received = Vec::new();
                let mut buf = [0u8; 8192];
                let mut expected: Option<usize> = None;
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
                            expected = Some(headers_end + 4 + length);
                        }
                    }
                    if expected.is_some_and(|total| received.len() >= total) {
                        break;
                    }
                }
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

    (handle, format!("http://127.0.0.1:{}/v1", addr.port()))
}

/// Server pinned to `mode` against the fixture, with a 1M window so the
/// pinned tier is never over budget.
async fn pinned_server(mode: &str, endpoint: &str) -> (tempfile::TempDir, EvolveServer) {
    let (dir, graph) = fixture();
    let mut config = Config::default();
    config.context_length = 1_000_000;
    config.context_mode = mode.to_string();
    config.endpoint = endpoint.to_string();
    let server = EvolveServer::with_config(graph, dir.path(), &config).unwrap();
    (dir, server)
}

async fn reviewed_document_hash(server: &EvolveServer) -> String {
    let (status, document) = get_json(server, "/api/ide/document?path=src/reviewed.rs").await;
    assert_eq!(status, StatusCode::OK);
    document["hash"].as_str().unwrap().to_string()
}

/// POST /api/assistant/review with scope active_context and return the JSON.
async fn review(server: &EvolveServer, mode: &str) -> Value {
    let expected_hash = reviewed_document_hash(server).await;
    let (status, body) = post_json(
        server,
        "/api/assistant/review",
        json!({
            "path": "src/reviewed.rs",
            "question": "Review this file",
            "expected_hash": expected_hash,
            "mode": mode,
            "scope": "active_context"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "review failed: {body}");
    serde_json::from_str(&body).unwrap()
}

/// Total shipped evidence characters in a review response.
fn review_chars(review: &Value) -> usize {
    review["review"]["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["excerpt"].as_str().unwrap().len())
        .sum()
}

#[tokio::test]
async fn lite_mode_ships_less_evidence_than_full_mode() {
    let (_mock, endpoint) = spawn_mock_review_server().await;
    let (_lite_dir, lite) = pinned_server("lite", &endpoint).await;
    let (_full_dir, full) = pinned_server("full", &endpoint).await;

    let lite_chars = review_chars(&review(&lite, "lite").await);
    let full_chars = review_chars(&review(&full, "full").await);

    assert!(
        lite_chars < full_chars,
        "lite evidence ({lite_chars}) must be smaller than full ({full_chars})"
    );
}

#[tokio::test]
async fn preview_and_review_share_content_hash() {
    let (_mock, endpoint) = spawn_mock_review_server().await;
    let (_dir, server) = pinned_server("lite", &endpoint).await;
    let expected_hash = reviewed_document_hash(&server).await;

    let (status, body) = post_json(
        &server,
        "/api/assistant/evidence/preview",
        json!({
            "path": "src/reviewed.rs",
            "expected_hash": expected_hash,
            "mode": "lite",
            "scope": "active_context"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let preview: Value = serde_json::from_str(&body).unwrap();
    let review = review(&server, "lite").await;

    let preview_hash = preview["content_hash"].as_str().unwrap();
    let review_hash = review["context"]["content_hash"].as_str().unwrap();
    assert_eq!(preview_hash, review_hash);

    // Both describe the cached envelope the context endpoint reports.
    let (status, context) = get_json(&server, "/api/context").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview_hash, context["envelope_hash"].as_str().unwrap());
}

#[tokio::test]
async fn selected_document_stays_full_fidelity_in_lite_mode() {
    let (_mock, endpoint) = spawn_mock_review_server().await;
    let (_dir, server) = pinned_server("lite", &endpoint).await;

    let review = review(&server, "lite").await;
    let excerpts = review["review"]["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["excerpt"].as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        excerpts.contains("let marker = 41 + 1;"),
        "the reviewed file's body line must ship verbatim in lite mode"
    );
    assert!(
        excerpts.contains("pub fn add_one"),
        "the neighbor skeleton must still ship signatures"
    );
    assert!(
        !excerpts.contains("acc += i * 2;"),
        "the neighbor's body lines must be skeletonized away in lite mode"
    );
}

/// Regression: graph node paths may be absolute (the selected-node lookup and
/// the fresh-read branch both normalize via `graph_logical_path`, but the
/// envelope stores the raw node path). Without normalizing the projected
/// document's path, the selected document's envelope twin bypasses the
/// `logical_paths` dedup and the reviewed file ships twice.
#[tokio::test]
async fn absolute_node_path_envelope_twin_does_not_duplicate_selected_document() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("reviewed.rs"), REVIEWED_RS).unwrap();
    std::fs::write(src.join("neighbor.rs"), NEIGHBOR_RS).unwrap();
    let mut reviewed = Node::code(
        "crate::reviewed",
        src.join("reviewed.rs").to_string_lossy().as_ref(),
    );
    reviewed.tokens = selfware::token_count::estimate_content_tokens(REVIEWED_RS);
    let mut neighbor = Node::code("crate::neighbor", "src/neighbor.rs");
    neighbor.tokens = selfware::token_count::estimate_content_tokens(NEIGHBOR_RS);
    let graph = Graph {
        nodes: vec![reviewed, neighbor],
        edges: vec![edge("crate::reviewed", "crate::neighbor")],
    };
    let mut config = Config::default();
    config.context_length = 1_000_000;
    config.context_mode = "full".to_string();
    let server = EvolveServer::with_config(graph, dir.path(), &config).unwrap();
    let expected_hash = reviewed_document_hash(&server).await;

    let (status, body) = post_json(
        &server,
        "/api/assistant/evidence/preview",
        json!({
            "path": "src/reviewed.rs",
            "expected_hash": expected_hash,
            "mode": "full",
            "scope": "active_context"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "preview failed: {body}");
    let preview: Value = serde_json::from_str(&body).unwrap();

    assert_eq!(
        preview["candidate_files"].as_u64().unwrap(),
        2,
        "the selected document's envelope twin must dedup, not ship twice: {preview}"
    );
    let absolute_path = src.join("reviewed.rs").to_string_lossy().to_string();
    assert!(
        preview["manifest"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["path"].as_str() != Some(absolute_path.as_str())),
        "evidence chunks must emit the normalized logical path, not the raw node path: {preview}"
    );
}
