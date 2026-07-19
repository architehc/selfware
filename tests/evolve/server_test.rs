use axum::http::StatusCode;
use serde_json::{json, Value};
use selfware::evolve::server::EvolveServer;
use selfware::evolve::{Graph, Node};

use crate::{edge, get_json, get_text, post_json, sample_graph};

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
async fn test_api_graph_endpoint_returns_full_graph() {
    let server = EvolveServer::new(sample_graph());
    let (status, json) = get_json(&server, "/api/graph").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(json["edges"].as_array().unwrap().len(), 2);
    assert_eq!(json["nodes"][0]["id"], "agent");
}

#[tokio::test]
async fn test_api_context_endpoint_returns_non_code_layers() {
    let server = EvolveServer::new(sample_graph());
    let (status, json) = get_json(&server, "/api/context").await;
    assert_eq!(status, StatusCode::OK);
    let nodes = json["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["id"], "cluster-abc");
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
    assert_eq!(json["included"].as_array().unwrap().len(), 2);
    assert!(json["estimated_tokens"].as_u64().unwrap() >= 1200);
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
    assert!(json["explanation"].as_str().unwrap().contains("cluster-abc"));
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
    assert_eq!(json["architecture"]["passed"], true);
    assert_eq!(json["architecture"]["errors"].as_array().unwrap().len(), 0);
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
async fn test_api_ide_write_then_read_round_trip() {
    let server = EvolveServer::new(sample_graph());
    let path = "src/evolve/.ide-write-test.tmp";
    let (status, _) = post_json(
        &server,
        "/api/ide/write",
        json!({ "path": path, "content": "// write test\n" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = get_text(&server, "/api/ide/read?path=src/evolve/.ide-write-test.tmp").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "// write test\n");
    std::fs::remove_file(path).ok();
}

#[tokio::test]
async fn test_api_ide_write_rejects_path_traversal() {
    let server = EvolveServer::new(sample_graph());
    let (status, _) = post_json(
        &server,
        "/api/ide/write",
        json!({ "path": "../escape.txt", "content": "nope" }),
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
    assert!(body.contains("<div id=\"file-tree\">"));
    assert!(body.contains("<div id=\"editor\">"));
    assert!(body.contains("monaco-editor"));
    assert!(body.contains("/app.js"));
}

#[tokio::test]
async fn test_app_js_contains_editor_panel_code() {
    let server = EvolveServer::new(sample_graph());
    let (status, body) = get_text(&server, "/app.js").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("/api/ide/files"));
    assert!(body.contains("/api/ide/read"));
    assert!(body.contains("/api/ide/write"));
    assert!(body.contains("monaco.editor.create"));
}

#[tokio::test]
async fn test_root_serves_index_html() {
    let server = EvolveServer::new(sample_graph());
    let (status, body) = get_text(&server, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<div id=\"graph\">"));
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
    assert!(body.contains("#graph"));
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
        if let Ok(resp) = reqwest::get(format!("http://127.0.0.1:{}/api/graph", port)).await {
            graph = Some(resp.json::<Value>().await.unwrap());
            break;
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
        edges: vec![edge("a", "b"), edge("b", "a")],
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
    assert!(body.contains("<div id=\"graph\">"));
    assert!(body.contains("/app.js"));
}
