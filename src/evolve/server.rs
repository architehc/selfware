//! Web server exposing the evolve graph as JSON and the D3 frontend.
//!
//! Endpoints:
//! - `GET /api/graph`           — the full layered graph
//! - `GET /api/context`         — concept and preset layer nodes (context sources)
//!                                plus the composer's included set and token estimate
//! - `GET /api/persona[?id=X]`  — grounded explanations for components
//! - `GET /api/actions`         — actionable edges plus suggested evolve branches
//! - `GET /api/gates`           — architecture gate results
//! - `GET /api/ontology/validate` — structural validation (cycles, dangling
//!                                  edges, isolated nodes)
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
    pub fn router(&self) -> Router {
        Router::new()
            .route("/api/graph", get(graph_handler))
            .route("/api/context", get(context_handler))
            .route("/api/persona", get(persona_handler))
            .route("/api/actions", get(actions_handler))
            .route("/api/gates", get(gates_handler))
            .route("/api/ontology/validate", get(ontology_validate_handler))
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

/// Structural validation of the graph: cycles, dangling edges, and isolated
/// nodes. `valid` is true only when all three categories are empty.
async fn ontology_validate_handler(State(server): State<Arc<EvolveServer>>) -> Json<Value> {
    Json(serde_json::to_value(super::validate_graph(&server.graph))
        .expect("validation report serialization cannot fail"))
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
