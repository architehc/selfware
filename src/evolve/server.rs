//! Web server exposing the evolve graph as JSON and the D3 frontend.
//!
//! Endpoints:
//! - `GET /api/graph`           — the full layered graph
//! - `GET /api/context`         — concept and preset layer nodes (context sources)
//!                                plus the composer's included set and token estimate
//! - `GET /api/persona[?id=X]`  — grounded explanations for components
//! - `GET /api/actions`         — actionable edges plus suggested evolve branches
//! - `GET /api/gates`           — architecture gate results
//! - `GET /api/ide/files`       — file explorer listing of `src/`
//! - `GET /api/ide/read?path=X` — contents of a source file (plain text)
//! - `POST /api/ide/write`      — write `{path, content}` back to disk
//! - `GET /`                    — static D3 visualization frontend (`src/evolve/web/`)
//! - `GET /editor.html`         — Monaco-based IDE editor panel (same static dir)

use anyhow::Result;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

use super::{
    Action, ActionEngine, ComponentPersona, ContextComposer, ContextMode, EdgeType, EvolutionLoop,
    Gatekeeper, Graph, IdeEngine, NodeLayer, OntologyStore,
};

/// Directory containing the static frontend assets.
const WEB_DIR: &str = "src/evolve/web";

/// Path where the ontology store persists the graph.
const ONTOLOGY_PATH: &str = ".selfware/evolve-graph.yaml";

/// Serves the evolve graph and its subsystems over HTTP.
#[derive(Clone)]
pub struct EvolveServer {
    graph: Arc<Graph>,
    composer: Arc<ContextComposer>,
    actions: Arc<ActionEngine>,
    gates: Arc<Gatekeeper>,
    persona: Arc<ComponentPersona>,
    ide: Arc<IdeEngine>,
    #[allow(dead_code)] // Held for re-analysis after actions; wired to an endpoint later.
    loop_: Arc<EvolutionLoop>,
    ontology: Arc<OntologyStore>,
}

impl EvolveServer {
    pub fn new(graph: Graph) -> Self {
        let mut composer = ContextComposer::new(graph.clone());
        composer.set_mode(ContextMode::Full);
        Self {
            graph: Arc::new(graph.clone()),
            composer: Arc::new(composer),
            actions: Arc::new(ActionEngine::new()),
            gates: Arc::new(Gatekeeper::new()),
            persona: Arc::new(ComponentPersona::new()),
            ide: Arc::new(IdeEngine::new("src")),
            loop_: Arc::new(EvolutionLoop::new(graph)),
            ontology: Arc::new(OntologyStore::new(ONTOLOGY_PATH)),
        }
    }

    /// Returns the full graph as pretty-printed JSON.
    pub async fn graph_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(&*self.graph)?)
    }

    /// Persists the graph through the ontology store.
    pub fn save_graph(&self) -> Result<()> {
        self.ontology.save(&self.graph)
    }

    /// Builds the HTTP router with the API endpoints and static frontend.
    fn router(&self) -> Router {
        Router::new()
            .route("/api/graph", get(graph_handler))
            .route("/api/context", get(context_handler))
            .route("/api/persona", get(persona_handler))
            .route("/api/actions", get(actions_handler))
            .route("/api/gates", get(gates_handler))
            .route("/api/ide/files", get(ide_files_handler))
            .route("/api/ide/read", get(ide_read_handler))
            .route("/api/ide/write", post(ide_write_handler))
            .with_state(Arc::new(self.clone()))
            .fallback_service(ServeDir::new(WEB_DIR))
    }

    /// Binds on 127.0.0.1 and serves until the task is shut down.
    pub async fn start(&self, port: u16) -> Result<()> {
        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr).await?;
        println!("Evolve server listening on http://{}", addr);
        axum::serve(listener, self.router()).await?;
        Ok(())
    }
}

async fn graph_handler(State(server): State<Arc<EvolveServer>>) -> Json<Value> {
    Json(serde_json::to_value(&*server.graph).expect("graph serialization cannot fail"))
}

/// Concept and preset layers — the sources the context selector composes from —
/// plus the composer's currently included set and estimated token cost.
async fn context_handler(State(server): State<Arc<EvolveServer>>) -> Json<Value> {
    let nodes: Vec<&super::Node> = server
        .graph
        .nodes
        .iter()
        .filter(|n| n.layer != NodeLayer::Code)
        .collect();
    Json(json!({
        "nodes": nodes,
        "included": server.composer.included_nodes(),
        "estimated_tokens": server.composer.estimate_tokens(),
    }))
}

/// Grounded explanations from the persona generator. With `?id=X`, explains a
/// single component (404 when unknown); otherwise explains every node.
async fn persona_handler(
    State(server): State<Arc<EvolveServer>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let explain = |node: &super::Node| -> Result<String, (StatusCode, Json<Value>)> {
        server.persona.explain(node).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })
    };

    if let Some(id) = params.get("id") {
        let node = server
            .graph
            .nodes
            .iter()
            .find(|n| &n.id == id)
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": format!("unknown component: {id}") })),
                )
            })?;
        return Ok(Json(json!({ "id": node.id, "explanation": explain(node)? })));
    }

    let personas: Result<Vec<Value>, (StatusCode, Json<Value>)> = server
        .graph
        .nodes
        .iter()
        .map(|n| Ok(json!({ "id": n.id, "explanation": explain(n)? })))
        .collect();
    Ok(Json(json!({ "personas": personas? })))
}

/// Edges representing actionable findings (duplicates and near-duplicates),
/// each with a suggested `Extend` branch name from the action engine.
async fn actions_handler(State(server): State<Arc<EvolveServer>>) -> Json<Value> {
    let edges: Vec<&super::Edge> = server
        .graph
        .edges
        .iter()
        .filter(|e| matches!(e.edge_type, EdgeType::DuplicateOf | EdgeType::SimilarTo))
        .collect();
    let suggested: Vec<Value> = edges
        .iter()
        .filter_map(|e| {
            let action = Action::Extend {
                component: e.to.clone(),
            };
            server.actions.execute(&action).ok().map(|r| {
                json!({
                    "action": "Extend",
                    "component": e.to,
                    "branch": r.branch,
                    "message": r.message,
                })
            })
        })
        .collect();
    Json(json!({ "edges": edges, "suggested": suggested }))
}

/// Architecture gate results. The code gate (`cargo check`) is intentionally
/// not run per-request; it is exercised before applying actions instead.
async fn gates_handler(State(server): State<Arc<EvolveServer>>) -> Json<Value> {
    let arch = server
        .gates
        .check_architecture_gates(&server.graph)
        .unwrap_or(super::GateResult {
            passed: false,
            errors: vec!["architecture gate failed to run".to_string()],
        });
    Json(json!({
        "passed": arch.passed,
        "architecture": { "passed": arch.passed, "errors": arch.errors },
    }))
}

/// File explorer listing of the entries under `src/`.
async fn ide_files_handler(
    State(server): State<Arc<EvolveServer>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let files = server.ide.list_files().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    Ok(Json(json!(files)))
}

/// Contents of a source file as plain text. `?path=` may be given with or
/// without the `src/` prefix; unknown files return 404.
async fn ide_read_handler(
    State(server): State<Arc<EvolveServer>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<String, (StatusCode, Json<Value>)> {
    let path = params.get("path").ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "missing path query parameter" })),
        )
    })?;
    server.ide.read_file(path).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": e.to_string() })),
        )
    })
}

/// Request body for `POST /api/ide/write`.
#[derive(serde::Deserialize)]
struct IdeWriteRequest {
    path: String,
    content: String,
}

/// Writes file contents back to disk. `path` may be given with or without the
/// `src/` prefix; paths escaping the source root are rejected with 400.
async fn ide_write_handler(
    State(server): State<Arc<EvolveServer>>,
    Json(body): Json<IdeWriteRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    server.ide.write_file(&body.path, &body.content).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    Ok(Json(json!({ "path": body.path, "saved": true })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::evolve::{Edge, Node};

    fn sample_graph() -> Graph {
        Graph {
            nodes: vec![
                Node::code("agent", "src/agent"),
                Node {
                    id: "cluster-abc".to_string(),
                    layer: NodeLayer::Concept,
                    path: None,
                    tokens: 1200,
                    lines: 0,
                    files: 4,
                    coverage: None,
                    dead_code_ratio: None,
                    warning_count: None,
                    complexity: None,
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

    async fn post_json(server: &EvolveServer, uri: &str, body: Value) -> (StatusCode, String) {
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
        (status, String::from_utf8(body.to_vec()).unwrap())
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
        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if let Ok(resp) = reqwest::get(format!("http://127.0.0.1:{}/api/graph", port)).await {
                graph = Some(resp.json::<Value>().await.unwrap());
                break;
            }
        }
        handle.abort();

        let graph = graph.expect("server did not respond on /api/graph");
        assert_eq!(graph["nodes"].as_array().unwrap().len(), 2);
    }
}
