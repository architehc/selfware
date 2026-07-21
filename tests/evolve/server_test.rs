use axum::body::Body;
use axum::http::{Request, StatusCode};
use selfware::config::Config;
use selfware::evolve::server::EvolveServer;
use selfware::evolve::{Edge, EdgeType, Graph, Node, NodeLayer, OntologyStore};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::{edge, get_json, get_json_auth, get_text, post_json, sample_graph};

async fn post_json_without_session(
    server: &EvolveServer,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = server
        .router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

fn ast_contains_label(node: &Value, expected: &str) -> bool {
    node["label"].as_str() == Some(expected)
        || node["children"].as_array().is_some_and(|children| {
            children
                .iter()
                .any(|child| ast_contains_label(child, expected))
        })
}

#[tokio::test]
async fn test_server_returns_graph_json() {
    let graph = Graph {
        nodes: vec![],
        edges: vec![],
    };
    let server = EvolveServer::new(graph);
    let response = server.graph_json().await.unwrap();
    assert!(response.contains("nodes"));
}

#[tokio::test]
async fn test_persisted_ontology_keeps_semantic_edges_but_not_stale_derived_edges() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join(".selfware")).unwrap();
    let concept = Node {
        id: "concept::review".to_string(),
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
    };
    let persisted = Graph {
        nodes: vec![
            Node::code("crate::a", "src/a.rs"),
            Node::code("crate::b", "src/b.rs"),
            concept.clone(),
        ],
        edges: vec![
            Edge {
                from: "crate::a".to_string(),
                to: "crate::b".to_string(),
                edge_type: EdgeType::DuplicateOf,
            },
            Edge {
                from: "concept::review".to_string(),
                to: "crate::a".to_string(),
                edge_type: EdgeType::Influences,
            },
        ],
    };
    OntologyStore::new(project.path().join(".selfware/evolve-graph.yaml"))
        .save(&persisted)
        .unwrap();
    let derived = Graph {
        nodes: vec![
            Node::code("crate::a", "src/a.rs"),
            Node::code("crate::b", "src/b.rs"),
        ],
        edges: vec![],
    };

    let server = EvolveServer::with_config(derived, project.path(), &Config::default()).unwrap();
    let graph: Graph = serde_json::from_str(&server.graph_json().await.unwrap()).unwrap();

    assert!(graph.nodes.iter().any(|node| node.id == concept.id));
    assert!(graph.edges.iter().any(|edge| {
        edge.from == "concept::review"
            && edge.to == "crate::a"
            && edge.edge_type == EdgeType::Influences
    }));
    assert!(!graph
        .edges
        .iter()
        .any(|edge| edge.edge_type == EdgeType::DuplicateOf));
}

#[tokio::test]
async fn test_api_graph_endpoint_returns_full_graph() {
    let server = EvolveServer::new(sample_graph());
    let (status, json) = get_json(&server, "/api/graph").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(json["edges"].as_array().unwrap().len(), 2);
    assert_eq!(json["nodes"][0]["id"], "agent");
}

#[tokio::test]
async fn test_server_applies_local_only_browser_security_headers() {
    let server = EvolveServer::new(sample_graph());
    let response = server
        .router()
        .oneshot(
            Request::builder()
                .uri("/api/graph")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let policy = response
        .headers()
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(policy.contains("script-src 'self'"));
    assert!(policy.contains("connect-src 'self'"));
    assert!(!policy.contains("http:"));
    assert!(!policy.contains("https:"));
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(response.headers()["x-frame-options"], "DENY");
    assert_eq!(response.headers()["cache-control"], "no-store");
}

#[tokio::test]
async fn test_api_context_endpoint_separates_code_from_non_code_layers() {
    let server = EvolveServer::new(sample_graph());
    let (status, json) = get_json(&server, "/api/context").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["mode"], "full");
    assert_eq!(json["production"]["nodes"], 1);
    assert_eq!(json["tests"]["nodes"], 0);
    assert_eq!(json["included"].as_array().unwrap(), &[json!("agent")]);
}

#[tokio::test]
async fn test_workspace_bootstraps_session_and_grounded_capabilities() {
    let server = EvolveServer::new(sample_graph());
    let (status, json) = get_json(&server, "/api/workspace").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["name"], "selfware");
    assert!(json["root"].as_str().unwrap().ends_with("/selfware"));
    assert_eq!(json["session_token"], server.session_token());
    assert!(!json["session_token"].as_str().unwrap().is_empty());
    assert!(json["model"].is_string());
    assert!(json["endpoint_host"].is_string());
    assert_eq!(json["graph"]["nodes"], 2);
    assert_eq!(json["graph"]["edges"], 2);
    assert_eq!(json["context"]["mode"], "full");
    assert_eq!(json["capabilities"]["checked_writes"], true);
    assert_eq!(json["capabilities"]["ast"], true);
    assert_eq!(
        json["capabilities"]["deterministic_structural_summary"],
        true
    );
    assert_eq!(
        json["capabilities"]["grounded_review_snapshot_binding"],
        true
    );
    assert_eq!(json["capabilities"]["deletion_preview"], true);
    assert_eq!(json["capabilities"]["deletion_execute"], false);
}

#[tokio::test]
async fn test_context_mode_requires_session_and_preserves_state_when_rejected() {
    let server = EvolveServer::new(sample_graph());
    let (status, json) =
        post_json_without_session(&server, "/api/context/mode", json!({ "mode": "lite" })).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(json["error"].as_str().unwrap().contains("session token"));
    let (status, context) = get_json(&server, "/api/context").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(context["mode"], "full");
}

#[tokio::test]
async fn test_context_mode_switches_full_and_full_extended_source_sets() {
    let mut production = Node::code("crate::agent", "src/agent.rs");
    production.tokens = 100;
    production.files = 1;
    let mut test = Node::test("test::agent", "tests/agent.rs");
    test.tokens = 30;
    test.files = 1;
    let mut example = Node::test("example::demo", "examples/demo.rs");
    example.tokens = 20;
    example.files = 1;
    let server = EvolveServer::new(Graph {
        nodes: vec![production, test, example],
        edges: vec![],
    });

    let (status, body) = post_json(
        &server,
        "/api/context/mode",
        json!({ "mode": "full_extended" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let extended: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(extended["mode"], "full_extended");
    assert_eq!(extended["estimated_tokens"], 150);
    assert_eq!(extended["production"]["nodes"], 1);
    assert_eq!(extended["tests"]["nodes"], 1);
    assert_eq!(extended["examples"]["nodes"], 1);
    assert_eq!(extended["included"].as_array().unwrap().len(), 3);

    let (status, persisted) = get_json_auth(&server, "/api/context").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(persisted, extended);

    let (status, body) = post_json(&server, "/api/context/mode", json!({ "mode": "full" })).await;
    assert_eq!(status, StatusCode::OK);
    let full: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(full["mode"], "full");
    assert_eq!(full["estimated_tokens"], 100);
    assert_eq!(full["included"], json!(["crate::agent"]));
    assert_eq!(full["tests"]["nodes"], 0);
    assert_eq!(full["examples"]["nodes"], 0);
}

#[tokio::test]
async fn test_api_actions_endpoint_returns_actionable_edges() {
    let server = EvolveServer::new(sample_graph());
    let (status, json) = get_json(&server, "/api/actions").await;
    assert_eq!(status, StatusCode::OK);
    let edges = json["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["edge_type"], "DuplicateOf");
}

#[tokio::test]
async fn test_api_actions_endpoint_suggests_branches() {
    let server = EvolveServer::new(sample_graph());
    let (status, json) = get_json(&server, "/api/actions").await;
    assert_eq!(status, StatusCode::OK);
    let suggested = json["suggested"].as_array().unwrap();
    assert_eq!(suggested.len(), 1);
    assert_eq!(suggested[0]["component"], "b.rs");
    assert!(suggested[0]["branch"]
        .as_str()
        .unwrap()
        .starts_with("evolve/extend-b.rs-"));
}

#[tokio::test]
async fn test_api_context_endpoint_includes_composer_state() {
    let server = EvolveServer::new(sample_graph());
    let (status, json) = get_json(&server, "/api/context").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["included"].as_array().unwrap().len(), 1);
    assert_eq!(json["production"]["nodes"], 1);
    assert_eq!(json["tests"]["nodes"], 0);
}

async fn actions_document_hash(server: &EvolveServer) -> String {
    let (status, document) = get_json(server, "/api/ide/document?path=src/evolve/actions.rs").await;
    assert_eq!(status, StatusCode::OK);
    document["hash"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_grounded_review_rejects_stale_context_mode_before_model_call() {
    let server = EvolveServer::new(sample_graph());
    let expected_hash = actions_document_hash(&server).await;
    let (status, body) = post_json(
        &server,
        "/api/assistant/review",
        json!({
            "path": "src/evolve/actions.rs",
            "question": "Review this file",
            "expected_hash": expected_hash,
            "mode": "full_extended",
            "scope": "selected_document"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body.contains("context changed"));
}

#[tokio::test]
async fn test_evidence_preview_is_model_free_and_reports_exact_scope() {
    let server = EvolveServer::new(sample_graph());
    let expected_hash = actions_document_hash(&server).await;
    let (status, body) = post_json(
        &server,
        "/api/assistant/evidence/preview",
        json!({
            "path": "src/evolve/actions.rs",
            "expected_hash": expected_hash,
            "mode": "full",
            "scope": "selected_document"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let value: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["scope"], "selected_document");
    assert_eq!(value["candidate_files"], 1);
    assert_eq!(value["evidence_files"], 1);
    assert_eq!(value["evidence_complete"], true);
    assert_eq!(value["selected_document_hash"], expected_hash);
    assert_eq!(value["graph_revision"].as_str().unwrap().len(), 64);
    assert!(value["manifest"].as_array().unwrap().iter().all(|item| {
        item["content_hash"].as_str().unwrap().len() == 64
            && item["start_line"].as_u64().unwrap() >= 1
    }));
}

#[tokio::test]
async fn test_active_context_preview_rejects_selected_test_excluded_by_full_mode() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("src")).unwrap();
    std::fs::create_dir_all(project.path().join("tests")).unwrap();
    std::fs::write(project.path().join("src/lib.rs"), "pub fn live() {}\n").unwrap();
    std::fs::write(
        project.path().join("tests/live_test.rs"),
        "#[test]\nfn live_test() {}\n",
    )
    .unwrap();
    let server = EvolveServer::for_project(
        Graph {
            nodes: vec![
                Node::code("crate", "src/lib.rs"),
                Node::test("test::live", "tests/live_test.rs"),
            ],
            edges: vec![],
        },
        project.path(),
    )
    .unwrap();
    let (status, document) = get_json(&server, "/api/ide/document?path=tests/live_test.rs").await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = post_json(
        &server,
        "/api/assistant/evidence/preview",
        json!({
            "path": "tests/live_test.rs",
            "expected_hash": document["hash"],
            "mode": "full",
            "scope": "active_context"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("excluded by full mode"));
}

#[tokio::test]
async fn test_evidence_preview_rejects_stale_graph_revision() {
    let server = EvolveServer::new(sample_graph());
    let expected_hash = actions_document_hash(&server).await;
    let (status, body) = post_json(
        &server,
        "/api/assistant/evidence/preview",
        json!({
            "path": "src/evolve/actions.rs",
            "expected_hash": expected_hash,
            "mode": "full",
            "graph_revision": "stale-revision",
            "scope": "selected_document"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body.contains("graph changed"));
}

#[tokio::test]
async fn test_evidence_preview_rejects_stale_document_hash() {
    let server = EvolveServer::new(sample_graph());
    let (status, body) = post_json(
        &server,
        "/api/assistant/evidence/preview",
        json!({
            "path": "src/evolve/actions.rs",
            "expected_hash": "stale-document-hash",
            "mode": "full",
            "scope": "selected_document"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body.contains("document changed"));
}

#[tokio::test]
async fn test_grounded_review_rejects_stale_document_hash_before_model_call() {
    let server = EvolveServer::new(sample_graph());
    let (status, body) = post_json(
        &server,
        "/api/assistant/review",
        json!({
            "path": "src/evolve/actions.rs",
            "question": "Review this file",
            "expected_hash": "stale-document-hash",
            "mode": "full",
            "scope": "selected_document"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body.contains("document changed"));
}

#[tokio::test]
async fn test_evidence_preview_requires_expected_document_hash() {
    let server = EvolveServer::new(sample_graph());
    let (status, _) = post_json(
        &server,
        "/api/assistant/evidence/preview",
        json!({
            "path": "src/evolve/actions.rs",
            "mode": "full",
            "scope": "selected_document"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_api_persona_endpoint_returns_all_explanations() {
    let server = EvolveServer::new(sample_graph());
    let (status, json) = get_json(&server, "/api/persona").await;
    assert_eq!(status, StatusCode::OK);
    let personas = json["personas"].as_array().unwrap();
    assert_eq!(personas.len(), 2);
    assert_eq!(personas[0]["id"], "agent");
    assert!(personas[0]["explanation"]
        .as_str()
        .unwrap()
        .contains("agent"));
}

#[tokio::test]
async fn test_api_persona_endpoint_with_id_query() {
    let server = EvolveServer::new(sample_graph());
    let (status, json) = get_json(&server, "/api/persona?id=cluster-abc").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["id"], "cluster-abc");
    assert!(json["explanation"]
        .as_str()
        .unwrap()
        .contains("cluster-abc"));
}

#[tokio::test]
async fn test_api_persona_unknown_id_returns_404() {
    let server = EvolveServer::new(sample_graph());
    let (status, _) = get_json(&server, "/api/persona?id=nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_api_gates_endpoint_returns_gate_results() {
    let server = EvolveServer::new(sample_graph());
    let (status, json) = get_json(&server, "/api/gates").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["passed"], false);
    assert_eq!(json["architecture"]["valid"], false);
    assert_eq!(
        json["architecture"]["dangling_edges"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn test_api_ide_files_endpoint_lists_src() {
    let server = EvolveServer::new(sample_graph());
    let (status, json) = get_json(&server, "/api/ide/files").await;
    assert_eq!(status, StatusCode::OK);
    let files = json.as_array().unwrap();
    assert!(files.iter().any(|f| f["path"] == "src/lib.rs"));
    assert!(files.iter().all(|f| f["is_dir"].is_boolean()));
    assert!(files.iter().all(|f| f["size"].is_number()));
}

#[tokio::test]
async fn test_api_ide_read_endpoint_returns_file_contents() {
    let server = EvolveServer::new(sample_graph());
    let (status, body) = get_text(&server, "/api/ide/read?path=src/lib.rs").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("pub mod evolve;"));
}

#[tokio::test]
async fn test_api_ide_read_missing_path_param_returns_400() {
    let server = EvolveServer::new(sample_graph());
    let (status, _) = get_json(&server, "/api/ide/read").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_api_ide_read_unknown_file_returns_404() {
    let server = EvolveServer::new(sample_graph());
    let (status, _) = get_json(&server, "/api/ide/read?path=src/nope.rs").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_api_document_and_ast_share_exact_snapshot_identity() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("src/nested")).unwrap();
    std::fs::write(
        project.path().join("src/nested/sample.rs"),
        "pub fn answer() -> usize {\n    42\n}\n",
    )
    .unwrap();
    let server = EvolveServer::for_project(Graph::default(), project.path()).unwrap();

    let (status, document) = get_json(&server, "/api/ide/document?path=src/nested/sample.rs").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(document["path"], "src/nested/sample.rs");
    assert_eq!(document["language"], "rust");
    assert_eq!(document["lines"], 3);
    assert_eq!(document["hash"].as_str().unwrap().len(), 64);
    assert_eq!(
        document["content"],
        "pub fn answer() -> usize {\n    42\n}\n"
    );

    let (status, ast) = get_json(&server, "/api/ide/ast?path=src/nested/sample.rs").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ast["path"], document["path"]);
    assert_eq!(ast["hash"], document["hash"]);
    assert_eq!(ast["ast"]["kind"], "source_file");
    assert_eq!(ast["ast"]["start_line"], 1);
    assert_eq!(ast["ast"]["start_column"], 1);
    assert_eq!(ast["ast"]["has_error"], false);
    assert!(ast_contains_label(&ast["ast"], "answer"));
}

#[tokio::test]
async fn test_api_ast_rejects_non_rust_documents() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("src")).unwrap();
    std::fs::write(
        project.path().join("src/readme.js"),
        "export const note = 'Notes';\n",
    )
    .unwrap();
    let server = EvolveServer::for_project(Graph::default(), project.path()).unwrap();

    let (status, json) = get_json(&server, "/api/ide/ast?path=src/readme.js").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json["error"].as_str().unwrap().contains("supports Rust"));
}

#[tokio::test]
async fn test_api_structural_summary_is_hash_bound_and_non_model_generated() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("src")).unwrap();
    std::fs::write(
        project.path().join("src/sample.rs"),
        "pub fn answer() -> usize {\n    42\n}\n",
    )
    .unwrap();
    let server = EvolveServer::for_project(Graph::default(), project.path()).unwrap();

    let (status, document) = get_json(&server, "/api/ide/document?path=src/sample.rs").await;
    assert_eq!(status, StatusCode::OK);
    let (status, summary) = get_json(&server, "/api/ide/summary?path=src/sample.rs").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(summary["path"], document["path"]);
    assert_eq!(summary["hash"], document["hash"]);
    assert_eq!(summary["summary"], "pub fn answer() -> usize { ... }");
    assert_eq!(summary["structural"]["complete"], true);
    assert_eq!(summary["structural"]["parse_has_error"], false);
    assert_eq!(summary["structural"]["evidence"][0]["start_line"], 1);
    assert_eq!(
        summary["structural"]["evidence"][0]["projection"],
        "body_elided"
    );
    assert_eq!(summary["grounding"]["parser"], "tree-sitter-rust");
    assert_eq!(summary["grounding"]["model_generated"], false);
    assert_eq!(summary["grounding"]["semantic_inference"], false);
    assert_eq!(summary["grounding"]["complete"], true);
}

#[tokio::test]
async fn test_api_structural_summary_rejects_non_rust_documents() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("src")).unwrap();
    std::fs::write(
        project.path().join("src/sample.js"),
        "export const value = 1;\n",
    )
    .unwrap();
    let server = EvolveServer::for_project(Graph::default(), project.path()).unwrap();

    let (status, json) = get_json(&server, "/api/ide/summary?path=src/sample.js").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json["error"].as_str().unwrap().contains("supports Rust"));
}

#[tokio::test]
async fn test_api_structural_summary_rejects_oversized_source() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("src")).unwrap();
    std::fs::write(
        project.path().join("src/large.rs"),
        vec![b' '; 2 * 1024 * 1024 + 1],
    )
    .unwrap();
    let server = EvolveServer::for_project(Graph::default(), project.path()).unwrap();

    let (status, json) = get_json(&server, "/api/ide/summary?path=src/large.rs").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json["error"].as_str().unwrap().contains("read limit"));
}

#[tokio::test]
async fn test_api_document_rejects_directories_as_client_errors() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("src/nested")).unwrap();
    let server = EvolveServer::for_project(Graph::default(), project.path()).unwrap();

    let (status, json) = get_json(&server, "/api/ide/document?path=src/nested").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("supported repository source set"));
}

#[tokio::test]
async fn test_api_ide_write_then_read_round_trip() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("src/evolve")).unwrap();
    std::fs::create_dir_all(project.path().join("tests")).unwrap();
    std::fs::create_dir_all(project.path().join("examples")).unwrap();
    std::fs::write(project.path().join("src/lib.rs"), "pub mod evolve;\n").unwrap();
    let server = EvolveServer::for_project(sample_graph(), project.path()).unwrap();
    let path = "src/evolve/write_test.rs";
    let (status, _) = post_json(
        &server,
        "/api/ide/write",
        json!({ "path": path, "content": "// write test\n", "expected_hash": "missing" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = get_text(&server, "/api/ide/read?path=src/evolve/write_test.rs").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "// write test\n");
}

#[tokio::test]
async fn test_api_ide_write_requires_session() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("src")).unwrap();
    let server = EvolveServer::for_project(Graph::default(), project.path()).unwrap();

    let (status, json) = post_json_without_session(
        &server,
        "/api/ide/write",
        json!({
            "path": "src/unauthorized.rs",
            "content": "pub fn unauthorized() {}\n",
            "expected_hash": "missing"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(json["error"].as_str().unwrap().contains("session token"));
    assert!(!project.path().join("src/unauthorized.rs").exists());
}

#[tokio::test]
async fn test_api_ide_write_rejects_stale_hash_without_overwriting_newer_content() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("src")).unwrap();
    let file = project.path().join("src/version.rs");
    std::fs::write(&file, "pub fn version() -> u8 { 1 }\n").unwrap();
    let server = EvolveServer::for_project(Graph::default(), project.path()).unwrap();

    let (_, initial) = get_json(&server, "/api/ide/document?path=src/version.rs").await;
    let initial_hash = initial["hash"].as_str().unwrap();
    let (status, body) = post_json(
        &server,
        "/api/ide/write",
        json!({
            "path": "src/version.rs",
            "content": "pub fn version() -> u8 { 2 }\n",
            "expected_hash": initial_hash
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let saved: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(saved["saved"], true);
    assert_eq!(saved["write"]["previous_hash"], initial_hash);
    assert_ne!(saved["write"]["hash"], initial_hash);
    assert_eq!(saved["write"]["created"], false);
    assert_eq!(saved["graph_revision"].as_str().unwrap().len(), 64);

    let (status, body) = post_json(
        &server,
        "/api/ide/write",
        json!({
            "path": "src/version.rs",
            "content": "pub fn version() -> u8 { 3 }\n",
            "expected_hash": initial_hash
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let conflict: Value = serde_json::from_str(&body).unwrap();
    assert!(conflict["error"].as_str().unwrap().contains("stale write"));
    assert_eq!(
        std::fs::read_to_string(file).unwrap(),
        "pub fn version() -> u8 { 2 }\n"
    );
}

#[tokio::test]
async fn test_api_ide_write_rejects_path_traversal() {
    let server = EvolveServer::new(sample_graph());
    let (status, _) = post_json(
        &server,
        "/api/ide/write",
        json!({ "path": "../escape.txt", "content": "nope", "expected_hash": "missing" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_api_ide_write_missing_fields_returns_client_error() {
    let server = EvolveServer::new(sample_graph());
    let (status, _) = post_json(&server, "/api/ide/write", json!({ "path": "src/x.rs" })).await;
    assert!(status.is_client_error());
}

#[tokio::test]
async fn test_static_editor_html_is_served() {
    let server = EvolveServer::new(sample_graph());
    let (status, body) = get_text(&server, "/editor.html").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("http-equiv=\"refresh\" content=\"0; url=/\""));
    assert!(!body.contains("<script"));
    assert!(body.contains("Open Selfware Evolve Workspace"));
}

#[tokio::test]
async fn test_app_js_contains_editor_panel_code() {
    let server = EvolveServer::new(sample_graph());
    let (status, body) = get_text(&server, "/app.js").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("/api/ide/files"));
    assert!(body.contains("/api/ide/document"));
    assert!(body.contains("/api/ide/write"));
    assert!(body.contains("monaco.editor.create"));
}

#[tokio::test]
async fn test_root_serves_index_html() {
    let server = EvolveServer::new(sample_graph());
    let (status, body) = get_text(&server, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<div id=\"graph-canvas\""));
}

#[tokio::test]
async fn test_static_app_js_is_served() {
    let server = EvolveServer::new(sample_graph());
    let (status, body) = get_text(&server, "/app.js").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("/api/graph"));
}

#[tokio::test]
async fn test_static_style_css_is_served() {
    let server = EvolveServer::new(sample_graph());
    let (status, body) = get_text(&server, "/style.css").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(".graph-canvas"));
}

#[tokio::test]
async fn test_start_serves_over_http() {
    // Find a free port, then release it for the server to bind.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let server = EvolveServer::new(sample_graph());
    let handle = tokio::spawn(async move { server.start(port).await });

    let mut graph = None;
    for _ in 0..100 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        match reqwest::get(format!("http://127.0.0.1:{}/api/graph", port)).await {
            Ok(resp) => {
                graph = Some(resp.json::<Value>().await.unwrap());
                break;
            }
            Err(e) => eprintln!("poll failed: {e:?}"),
        }
    }
    handle.abort();

    let graph = graph.expect("server did not respond on /api/graph");
    assert_eq!(graph["nodes"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_validate_endpoint_clean_graph_is_valid() {
    let graph = Graph {
        nodes: vec![Node::code("a", "src/a.rs"), Node::code("b", "src/b.rs")],
        edges: vec![edge("a", "b")],
    };
    let server = EvolveServer::new(graph);
    let (status, json) = get_json(&server, "/api/ontology/validate").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["valid"], true);
}

#[tokio::test]
async fn test_validate_endpoint_reports_problems() {
    let graph = Graph {
        nodes: vec![Node::code("agent", "src/agent")],
        edges: vec![edge("agent", "ghost")],
    };
    let server = EvolveServer::new(graph);
    let (status, json) = get_json(&server, "/api/ontology/validate").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["valid"], false);
    assert_eq!(json["dangling_edges"].as_array().unwrap().len(), 1);
    assert_eq!(json["isolated_nodes"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_validate_endpoint_detects_cycle() {
    let graph = Graph {
        nodes: vec![Node::code("a", "src/a.rs"), Node::code("b", "src/b.rs")],
        edges: vec![
            Edge {
                from: "a".to_string(),
                to: "b".to_string(),
                edge_type: EdgeType::Contains,
            },
            Edge {
                from: "b".to_string(),
                to: "a".to_string(),
                edge_type: EdgeType::Contains,
            },
        ],
    };
    let server = EvolveServer::new(graph);
    let (status, json) = get_json(&server, "/api/ontology/validate").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["valid"], false);
    assert_eq!(json["cycles"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_existing_endpoints_unchanged() {
    let graph = Graph {
        nodes: vec![Node::code("agent", "src/agent")],
        edges: vec![],
    };
    let server = EvolveServer::new(graph);
    let (status, json) = get_json(&server, "/api/graph").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["nodes"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_root_serves_graph_page() {
    let server = EvolveServer::new(Graph::default());
    let (status, body) = get_text(&server, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<div id=\"graph-canvas\""));
    assert!(body.contains("/app.js"));
}
