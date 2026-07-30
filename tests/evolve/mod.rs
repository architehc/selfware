//! Tests for the self-evolve feature (`src/evolve/`), separated from the
//! source files they cover. Each `X_test` module mirrors `src/evolve/X.rs`.
//!
//! This root also holds the common test utilities: shared graph fixtures and
//! HTTP request helpers for exercising the evolve server in-process.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use selfware::evolve::{Edge, EdgeType, EvolveServer, Graph, Node, NodeLayer};

mod actions_test;
// Apply staging requires shadow worktrees from `evolution::ast_tools`, which
// exists only with the self-improvement feature (mirrors src/evolve/mod.rs).
#[cfg(feature = "self-improvement")]
mod apply_isolation_test;
mod assistant_protocol_test;
mod assistant_test;
mod ast_test;
mod auto_context_fit_test;
mod context_test;
mod dedup_test;
mod deletion_test;
mod diagnostics_test;
mod envelope_evidence_test;
mod expansion_test;
mod git_test;
mod graph_test;
mod graphrag_test;
mod ide_test;
mod loop_test;
mod mod_test;
mod ontology_evolver_test;
mod ontology_test;
mod persona_test;
mod quality_test;
mod readiness_test;
mod server_test;
mod small_model_context_fit_test;
mod summary_test;
mod symbol_context_test;

/// Shorthand for a `DependsOn` edge between two node ids.
fn edge(from: &str, to: &str) -> Edge {
    Edge {
        from: from.to_string(),
        to: to.to_string(),
        edge_type: EdgeType::DependsOn,
    }
}

/// Concept-layer node with zeroed metrics (classification "concept").
pub fn concept_node(id: &str) -> Node {
    Node {
        id: id.to_string(),
        layer: NodeLayer::Concept,
        path: None,
        tokens: 0,
        lines: 0,
        files: 0,
        coverage: None,
        dead_code_ratio: None,
        warning_count: None,
        complexity: None,
        inline_test_ranges: 0,
        inline_test_lines: 0,
        inline_test_tokens: 0,
        classification: "concept".to_string(),
    }
}

/// Temp repo with one committed file; returns (dir, HEAD oid).
pub fn committed_repository() -> (tempfile::TempDir, String) {
    let project = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(project.path()).unwrap();
    std::fs::write(project.path().join("README.md"), "initial\n").unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("README.md")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = git2::Signature::now("Selfware Test", "selfware@example.test").unwrap();
    let head = repo
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            "initial commit",
            &tree,
            &[],
        )
        .unwrap()
        .to_string();
    (project, head)
}

/// Graph with one code node, one concept node, and duplicate/depends-on edges.
fn sample_graph() -> Graph {
    Graph {
        nodes: vec![
            Node::code("agent", "src/agent"),
            Node {
                tokens: 1200,
                files: 4,
                ..concept_node("cluster-abc")
            },
        ],
        edges: vec![
            Edge {
                from: "a.rs".to_string(),
                to: "b.rs".to_string(),
                edge_type: EdgeType::DuplicateOf,
            },
            Edge {
                from: "agent".to_string(),
                to: "config".to_string(),
                edge_type: EdgeType::DependsOn,
            },
        ],
    }
}

async fn get_json(server: &EvolveServer, uri: &str) -> (StatusCode, Value) {
    let response = server
        .router()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = serde_json::from_slice(&body).unwrap();
    (status, json)
}

async fn post_json(server: &EvolveServer, uri: &str, body: Value) -> (StatusCode, String) {
    let response = server
        .router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .header("x-selfware-session", server.session_token())
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

async fn get_json_auth(server: &EvolveServer, uri: &str) -> (StatusCode, Value) {
    let response = server
        .router()
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("x-selfware-session", server.session_token())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = serde_json::from_slice(&body).unwrap();
    (status, json)
}

async fn get_text(server: &EvolveServer, uri: &str) -> (StatusCode, String) {
    let response = server
        .router()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}
