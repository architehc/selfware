//! Local HTTP workspace for the self-evolve graph, IDE, and grounded actions.

use anyhow::{Context, Result};
use axum::{
    extract::{Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tokio::net::TcpListener;
use tokio::sync::Mutex as AsyncMutex;
use tower_http::services::ServeDir;

use super::assistant::{
    evidence_from_document, evidence_from_document_excluding_ranges, GroundedAssistant,
};
use super::deletion::preview_deletion;
use super::diagnostics::{AnalysisKind, DiagnosticsEngine};
use super::git::GitEngine;
use super::readiness::ReadinessEngine;
use super::{
    validate_graph, Action, ActionEngine, AstAnalyzer, ComponentPersona, ContextComposer,
    ContextMode, DocumentSnapshot, EdgeType, Graph, GraphBuilder, IdeEngine, NodeLayer,
    OntologyStore,
};
use crate::config::Config;

const WEB_DIR: &str = "src/evolve/web";
const SESSION_HEADER: &str = "x-selfware-session";
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self' data:; connect-src 'self'; worker-src 'self' blob:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'";

type ApiError = (StatusCode, Json<Value>);
type ApiResult<T> = std::result::Result<T, ApiError>;

#[derive(Clone)]
pub struct EvolveServer {
    graph: Arc<RwLock<Graph>>,
    composer: Arc<RwLock<ContextComposer>>,
    actions: Arc<ActionEngine>,
    persona: Arc<ComponentPersona>,
    ide: Arc<IdeEngine>,
    ast: Arc<AstAnalyzer>,
    diagnostics: Arc<DiagnosticsEngine>,
    git: Arc<GitEngine>,
    readiness: Arc<ReadinessEngine>,
    assistant: Arc<GroundedAssistant>,
    concept_index: Arc<super::ConceptIndex>,
    ontology: Option<Arc<OntologyStore>>,
    project_root: Arc<PathBuf>,
    configured_model: Arc<String>,
    endpoint_host: Arc<String>,
    context_length: usize,
    evidence_char_budget: usize,
    session_token: Arc<String>,
    write_lock: Arc<AsyncMutex<()>>,
    analysis_lock: Arc<AsyncMutex<()>>,
    assistant_lock: Arc<AsyncMutex<()>>,
    git_lock: Arc<AsyncMutex<()>>,
}

impl EvolveServer {
    /// Test and compatibility constructor using the repository root and default
    /// model configuration. No model request is made during construction.
    pub fn new(graph: Graph) -> Self {
        Self::build(graph, ".", &Config::default(), false)
            .expect("default evolve server configuration must be valid")
    }

    /// Construct an isolated server for an explicit project root without
    /// loading or persisting the repository ontology cache.
    pub fn for_project(graph: Graph, project_root: impl AsRef<Path>) -> Result<Self> {
        Self::build(graph, project_root, &Config::default(), false)
    }

    pub fn with_config(
        derived_graph: Graph,
        project_root: impl AsRef<Path>,
        config: &Config,
    ) -> Result<Self> {
        Self::build(derived_graph, project_root, config, true)
    }

    fn build(
        derived_graph: Graph,
        project_root: impl AsRef<Path>,
        config: &Config,
        persist_ontology: bool,
    ) -> Result<Self> {
        let project_root = std::fs::canonicalize(project_root.as_ref()).with_context(|| {
            format!("invalid project root: {}", project_root.as_ref().display())
        })?;
        let ontology_path = project_root.join(".selfware/evolve-graph.yaml");
        let ontology = OntologyStore::new(&ontology_path);
        let graph = if persist_ontology && ontology_path.exists() {
            let persisted = ontology
                .load()
                .with_context(|| format!("failed to load {}", ontology_path.display()))?;
            merge_derived_with_ontology(derived_graph, persisted)
        } else {
            derived_graph
        };
        let mut composer = ContextComposer::new(graph.clone());
        composer.set_mode(ContextMode::Full);
        let endpoint_host = url::Url::parse(&config.endpoint)
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
            .unwrap_or_else(|| "configured endpoint".to_string());
        let output_reserve = config.max_tokens.min(config.context_length / 4);
        let evidence_char_budget = config
            .context_length
            .saturating_sub(output_reserve)
            .saturating_sub(16_384)
            .saturating_mul(2)
            .clamp(32_000, 2_000_000);

        let concept_index = super::ConceptIndex::build(project_root.join("src"))
            .unwrap_or_else(|_| super::ConceptIndex::empty());

        Ok(Self {
            graph: Arc::new(RwLock::new(graph)),
            composer: Arc::new(RwLock::new(composer)),
            actions: Arc::new(ActionEngine::new()),
            persona: Arc::new(ComponentPersona::new()),
            ide: Arc::new(IdeEngine::for_project(&project_root)),
            ast: Arc::new(AstAnalyzer::new()),
            diagnostics: Arc::new(DiagnosticsEngine::new(&project_root)),
            git: Arc::new(GitEngine::new(&project_root)),
            readiness: Arc::new(ReadinessEngine::new(&project_root)),
            assistant: Arc::new(GroundedAssistant::new(config)?),
            concept_index: Arc::new(concept_index),
            ontology: persist_ontology.then(|| Arc::new(ontology)),
            project_root: Arc::new(project_root),
            configured_model: Arc::new(config.model.clone()),
            endpoint_host: Arc::new(endpoint_host),
            context_length: config.context_length,
            evidence_char_budget,
            session_token: Arc::new(uuid::Uuid::new_v4().to_string()),
            write_lock: Arc::new(AsyncMutex::new(())),
            analysis_lock: Arc::new(AsyncMutex::new(())),
            assistant_lock: Arc::new(AsyncMutex::new(())),
            git_lock: Arc::new(AsyncMutex::new(())),
        })
    }

    pub fn session_token(&self) -> &str {
        self.session_token.as_str()
    }

    pub async fn graph_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(&self.graph_snapshot()?)?)
    }

    pub fn save_graph(&self) -> Result<()> {
        if let Some(ontology) = &self.ontology {
            ontology.save(&self.graph_snapshot()?)?;
        }
        Ok(())
    }

    pub fn router(&self) -> Router {
        Router::new()
            .route("/api/workspace", get(workspace_handler))
            .route("/api/graph", get(graph_handler))
            .route("/api/context", get(context_handler))
            .route("/api/context/mode", post(context_mode_handler))
            .route("/api/context/sizes", get(context_sizes_handler))
            .route("/api/analysis/duplicate-functions", get(duplicate_fns_handler))
            .route("/api/analysis/dead-code", get(dead_code_handler))
            .route("/api/persona", get(persona_handler))
            .route("/api/xray", get(xray_handler))
            .route("/api/xray/hubs", get(xray_hubs_handler))
            .route("/api/actions", get(actions_handler))
            .route(
                "/api/actions/deletion/preview",
                post(deletion_preview_handler),
            )
            .route("/api/gates", get(gates_handler))
            .route("/api/ontology/validate", get(ontology_validate_handler))
            .route("/api/ide/files", get(ide_files_handler))
            .route("/api/ide/read", get(ide_read_handler))
            .route("/api/ide/document", get(ide_document_handler))
            .route("/api/ide/ast", get(ide_ast_handler))
            .route("/api/ide/summary", get(ide_summary_handler))
            .route("/api/ide/write", post(ide_write_handler))
            .route("/api/analysis/run", post(analysis_run_handler))
            .route("/api/readiness", get(readiness_handler))
            .route("/api/ide/recommendations", get(recommendations_handler))
            .route(
                "/api/assistant/evidence/preview",
                post(assistant_evidence_preview_handler),
            )
            .route("/api/assistant/review", post(assistant_review_handler))
            .route("/api/git/status", get(git_status_handler))
            .route("/api/git/branch", post(git_branch_handler))
            .with_state(Arc::new(self.clone()))
            .fallback_service(ServeDir::new(WEB_DIR).append_index_html_on_directories(true))
            .layer(middleware::from_fn(security_headers))
    }

    pub async fn start(&self, port: u16) -> Result<()> {
        let address = format!("127.0.0.1:{port}");
        let listener = TcpListener::bind(&address).await?;
        println!("Evolve workspace listening on http://{address}");
        axum::serve(listener, self.router()).await?;
        Ok(())
    }

    fn graph_snapshot(&self) -> Result<Graph> {
        self.graph
            .read()
            .map(|graph| graph.clone())
            .map_err(|_| anyhow::anyhow!("graph lock poisoned"))
    }

    fn context_summary(&self) -> Result<super::ContextSummary> {
        self.composer
            .read()
            .map(|composer| composer.summary())
            .map_err(|_| anyhow::anyhow!("context lock poisoned"))
    }

    fn refresh_graph(&self) -> Result<String> {
        let derived = GraphBuilder::new(self.project_root.join("src")).scan_src()?;
        let existing = self.graph_snapshot()?;
        let refreshed = merge_derived_with_ontology(derived, existing);
        let revision = graph_revision(&refreshed)?;

        *self
            .graph
            .write()
            .map_err(|_| anyhow::anyhow!("graph lock poisoned"))? = refreshed.clone();
        let mut current = self
            .composer
            .write()
            .map_err(|_| anyhow::anyhow!("context lock poisoned"))?;
        let current_mode = current.mode().clone();
        let mut composer = ContextComposer::new(refreshed);
        composer.set_mode(current_mode);
        *current = composer;
        self.save_graph()?;
        Ok(revision)
    }
}

async fn workspace_handler(State(server): State<Arc<EvolveServer>>) -> ApiResult<Json<Value>> {
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let context = server.context_summary().map_err(internal_error)?;
    let git = server.git.status().map_err(internal_error)?;
    let revision = graph_revision(&graph).map_err(internal_error)?;
    let workspace_name = server
        .project_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("selfware");
    let context_value = context_json(&context, server.context_length).map_err(internal_error)?;
    Ok(Json(json!({
        "name": workspace_name,
        "root": server.project_root.to_string_lossy(),
        "model": server.configured_model.as_str(),
        "endpoint_host": server.endpoint_host.as_str(),
        "context_length": server.context_length,
        "session_token": server.session_token.as_str(),
        "graph_revision": revision,
        "graph": { "nodes": graph.nodes.len(), "edges": graph.edges.len() },
        "context": context_value,
        "git": git,
        "capabilities": {
            "checked_writes": true,
            "compiler_diagnostics": true,
            "ast": true,
            "deterministic_structural_summary": true,
            "grounded_review": true,
            "grounded_review_snapshot_binding": true,
            "evidence_preview": true,
            "full_context_inline_test_filter": true,
            "branch_creation": true,
            "deletion_preview": true,
            "deletion_execute": false
        }
    })))
}

async fn graph_handler(State(server): State<Arc<EvolveServer>>) -> ApiResult<Json<Value>> {
    Ok(Json(
        serde_json::to_value(server.graph_snapshot().map_err(internal_error)?)
            .map_err(internal_error)?,
    ))
}

async fn context_handler(State(server): State<Arc<EvolveServer>>) -> ApiResult<Json<Value>> {
    let summary = server.context_summary().map_err(internal_error)?;
    Ok(Json(
        context_json(&summary, server.context_length).map_err(internal_error)?,
    ))
}

#[derive(Deserialize)]
struct ContextModeRequest {
    mode: String,
    preset: Option<String>,
}

/// Function-level duplicate/near-duplicate clones across the source tree —
/// the "excess code" that whole-file dedup misses. Read-only.
async fn duplicate_fns_handler(State(server): State<Arc<EvolveServer>>) -> ApiResult<Json<Value>> {
    let root = server.project_root.join("src");
    let pairs = tokio::task::spawn_blocking(move || super::FnDedupAnalyzer::new(root).find())
        .await
        .map_err(|e| internal_error(anyhow::anyhow!(e)))?
        .map_err(internal_error)?;
    let exact = pairs.iter().filter(|p| p.kind == "exact").count();
    Ok(Json(json!({
        "total": pairs.len(),
        "exact": exact,
        "near": pairs.len() - exact,
        "pairs": pairs,
    })))
}

/// Reachability-based dead code: symbols whose name appears only at their own
/// definition (text-based, so cfg-gated cross-platform code isn't false-flagged).
async fn dead_code_handler(State(server): State<Arc<EvolveServer>>) -> ApiResult<Json<Value>> {
    let root = server.project_root.as_ref().clone();
    let dead = tokio::task::spawn_blocking(move || super::DeadCodeAnalyzer::new(root).find())
        .await
        .map_err(|e| internal_error(anyhow::anyhow!(e)))?
        .map_err(internal_error)?;
    let confident = dead.iter().filter(|d| !d.cfg_gated).count();
    Ok(Json(json!({
        "total": dead.len(),
        "confident": confident,
        "cfg_gated": dead.len() - confident,
        "symbols": dead,
    })))
}

/// Node count and token size of every selectable context mode, so the picker
/// can show the cost of each option. Read-only; does not change the active mode.
async fn context_sizes_handler(State(server): State<Arc<EvolveServer>>) -> ApiResult<Json<Value>> {
    let sizes = {
        let composer = server
            .composer
            .read()
            .map_err(|_| internal_error(anyhow::anyhow!("context lock poisoned")))?;
        composer.mode_sizes()
    };
    Ok(Json(json!({
        "sizes": sizes,
        "context_length": server.context_length,
    })))
}

async fn context_mode_handler(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    Json(body): Json<ContextModeRequest>,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;
    let _assistant_guard = server.assistant_lock.lock().await;
    let mode = match body.mode.as_str() {
        "lite" => ContextMode::Lite,
        "full" => ContextMode::Full,
        "full_extended" => ContextMode::FullExtended,
        "preset" => ContextMode::Preset(
            body.preset
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| bad_request("preset mode requires a preset name"))?,
        ),
        _ => return Err(bad_request("unknown context mode")),
    };
    let summary = {
        let mut composer = server
            .composer
            .write()
            .map_err(|_| internal_error(anyhow::anyhow!("context lock poisoned")))?;
        composer.set_mode(mode);
        composer.summary()
    };
    Ok(Json(
        context_json(&summary, server.context_length).map_err(internal_error)?,
    ))
}

fn context_json(summary: &super::ContextSummary, context_length: usize) -> Result<Value> {
    let mut value = serde_json::to_value(summary)?;
    if let Some(object) = value.as_object_mut() {
        object.insert("context_length".to_string(), json!(context_length));
        object.insert(
            "fits_context_window".to_string(),
            json!(summary.estimated_tokens <= context_length),
        );
        object.insert(
            "overflow_tokens".to_string(),
            json!(summary.estimated_tokens.saturating_sub(context_length)),
        );
    }
    Ok(value)
}

async fn persona_handler(
    State(server): State<Arc<EvolveServer>>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let graph = server.graph_snapshot().map_err(internal_error)?;
    if let Some(id) = params.get("id") {
        let node = graph
            .nodes
            .iter()
            .find(|node| &node.id == id)
            .ok_or_else(|| not_found(format!("unknown component: {id}")))?;
        let explanation = server.persona.explain(node).map_err(internal_error)?;
        return Ok(Json(json!({ "id": node.id, "explanation": explanation })));
    }
    let personas = graph
        .nodes
        .iter()
        .map(|node| {
            server
                .persona
                .explain(node)
                .map(|explanation| json!({ "id": node.id, "explanation": explanation }))
        })
        .collect::<Result<Vec<_>>>()
        .map_err(internal_error)?;
    Ok(Json(json!({ "personas": personas })))
}

/// Ontology x-ray for a selected concept: `/api/xray?concept=Tool`.
async fn xray_handler(
    State(server): State<Arc<EvolveServer>>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let concept = params
        .get("concept")
        .ok_or_else(|| not_found("missing 'concept' query parameter"))?;
    let xray = server
        .concept_index
        .xray(concept)
        .ok_or_else(|| not_found(format!("'{concept}' is not a defined type")))?;
    Ok(Json(serde_json::to_value(xray).map_err(internal_error)?))
}

/// Ontology hubs (most-connected concepts): `/api/xray/hubs?limit=20`.
async fn xray_hubs_handler(
    State(server): State<Arc<EvolveServer>>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let limit = params
        .get("limit")
        .and_then(|value| value.parse().ok())
        .unwrap_or(20);
    let hubs = server.concept_index.hubs(limit);
    Ok(Json(json!({
        "concept_count": server.concept_index.concept_count(),
        "hubs": hubs,
    })))
}

async fn actions_handler(State(server): State<Arc<EvolveServer>>) -> ApiResult<Json<Value>> {
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let edges = graph
        .edges
        .iter()
        .filter(|edge| matches!(edge.edge_type, EdgeType::DuplicateOf | EdgeType::SimilarTo))
        .collect::<Vec<_>>();
    let suggested = edges
        .iter()
        .filter_map(|edge| {
            let action = Action::Extend {
                component: edge.to.clone(),
            };
            server.actions.propose(&action).ok().map(|result| {
                json!({
                    "action": "extend",
                    "component": edge.to,
                    "branch": result.branch,
                    "message": result.message,
                    "lifecycle": "proposed"
                })
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "edges": edges,
        "suggested": suggested,
        "available": ["inspect", "open_file", "set_context", "grounded_review", "preview_source_deletion"],
        "disabled": {
            "stage_source_deletion": "only non-mutating impact preview is implemented",
            "execute_source_deletion": "impact evidence and rollback execution are not complete",
            "automatic_merge": "user review is required"
        }
    })))
}

#[derive(Deserialize)]
struct DeletionPreviewRequest {
    node_id: String,
}

async fn deletion_preview_handler(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    Json(body): Json<DeletionPreviewRequest>,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;
    let _workspace_guard = server.write_lock.lock().await;
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let preview = preview_deletion(&graph, &server.ide, &body.node_id).map_err(bad_action_error)?;
    Ok(Json(serde_json::to_value(preview).map_err(internal_error)?))
}

async fn gates_handler(State(server): State<Arc<EvolveServer>>) -> ApiResult<Json<Value>> {
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let validation = validate_graph(&graph);
    Ok(Json(json!({
        "passed": validation.valid,
        "architecture": validation
    })))
}

async fn ontology_validate_handler(
    State(server): State<Arc<EvolveServer>>,
) -> ApiResult<Json<Value>> {
    let graph = server.graph_snapshot().map_err(internal_error)?;
    Ok(Json(
        serde_json::to_value(validate_graph(&graph)).map_err(internal_error)?,
    ))
}

async fn ide_files_handler(State(server): State<Arc<EvolveServer>>) -> ApiResult<Json<Value>> {
    Ok(Json(json!(server
        .ide
        .list_files()
        .map_err(internal_error)?)))
}

async fn ide_read_handler(
    State(server): State<Arc<EvolveServer>>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<String> {
    let path = required_path(&params)?;
    server.ide.read_file(path).map_err(document_read_error)
}

async fn ide_document_handler(
    State(server): State<Arc<EvolveServer>>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let path = required_path(&params)?;
    let document = server
        .ide
        .read_document(path)
        .map_err(document_read_error)?;
    Ok(Json(
        serde_json::to_value(document).map_err(internal_error)?,
    ))
}

async fn ide_ast_handler(
    State(server): State<Arc<EvolveServer>>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let path = required_path(&params)?;
    let document = server
        .ide
        .read_document(path)
        .map_err(document_read_error)?;
    if document.language != "rust" {
        return Err(bad_request("AST view currently supports Rust documents"));
    }
    let ast = server
        .ast
        .parse_source(&document.content)
        .map_err(bad_action_error)?;
    Ok(Json(json!({
        "path": document.path,
        "hash": document.hash,
        "ast": ast
    })))
}

async fn ide_summary_handler(
    State(server): State<Arc<EvolveServer>>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let path = required_path(&params)?;
    const MAX_SUMMARY_SOURCE_BYTES: usize = 2 * 1024 * 1024;
    let document = server
        .ide
        .read_document_limited(path, MAX_SUMMARY_SOURCE_BYTES)
        .map_err(document_read_error)?;
    if document.language != "rust" {
        return Err(bad_request(
            "Summary view currently supports Rust documents",
        ));
    }
    let ast = server
        .ast
        .parse_source(&document.content)
        .map_err(bad_action_error)?;

    let summary = crate::evolve::summary::compile_structural_summary(&ast, &document.content);
    Ok(Json(json!({
        "path": document.path,
        "hash": document.hash,
        "summary": summary.outline.as_str(),
        "structural": &summary,
        "grounding": {
            "parser": "tree-sitter-rust",
            "projection": "source_declarations_attributes_and_comments",
            "model_generated": false,
            "semantic_inference": false,
            "complete": summary.complete
        }
    })))
}

#[derive(Deserialize)]
struct IdeWriteRequest {
    path: String,
    content: String,
    expected_hash: String,
}

async fn ide_write_handler(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    Json(body): Json<IdeWriteRequest>,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;
    let _write_guard = server.write_lock.lock().await;
    let _assistant_guard = server.assistant_lock.lock().await;
    let result = server
        .ide
        .write_file_checked(&body.path, &body.content, &body.expected_hash)
        .map_err(document_write_error)?;
    let refresh_server = server.clone();
    let refresh_result = tokio::task::spawn_blocking(move || refresh_server.refresh_graph())
        .await
        .map_err(internal_error)?;
    let (graph_revision, graph_refresh) = match refresh_result {
        Ok(graph_revision) => (
            Some(graph_revision.clone()),
            json!({
                "success": true,
                "graph_revision": graph_revision
            }),
        ),
        Err(error) => (
            None,
            json!({
                "success": false,
                "error": error.to_string()
            }),
        ),
    };
    Ok(Json(json!({
        "saved": true,
        "write": result,
        "graph_revision": graph_revision,
        "graph_refresh": graph_refresh
    })))
}

#[derive(Deserialize)]
struct AnalysisRunRequest {
    kind: AnalysisKind,
}

async fn analysis_run_handler(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    Json(body): Json<AnalysisRunRequest>,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;
    let _workspace_guard = server.write_lock.lock().await;
    let _analysis_guard = server.analysis_lock.lock().await;
    let report = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        server.diagnostics.run(body.kind),
    )
    .await
    .map_err(|_| internal_error("analysis timed out (300s); the build may still be running"))?
    .map_err(internal_error)?;
    Ok(Json(serde_json::to_value(report).map_err(internal_error)?))
}

async fn readiness_handler(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;
    let _workspace_guard = server.write_lock.lock().await;
    let _analysis_guard = server.analysis_lock.lock().await;
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let validation = validate_graph(&graph);
    let report = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        server.readiness.evaluate(&validation),
    )
    .await
    .map_err(|_| {
        internal_error("readiness evaluation timed out (300s) — analyses may still be running")
    })?
    .map_err(internal_error)?;
    Ok(Json(serde_json::to_value(report).map_err(internal_error)?))
}

async fn recommendations_handler(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;
    let _workspace_guard = server.write_lock.lock().await;
    let _analysis_guard = server.analysis_lock.lock().await;
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let validation = validate_graph(&graph);
    let report = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        server.readiness.evaluate(&validation),
    )
    .await
    .map_err(|_| {
        internal_error("readiness evaluation timed out (300s) — analyses may still be running")
    })?
    .map_err(internal_error)?;
    Ok(Json(
        serde_json::to_value(report.recommendations).map_err(internal_error)?,
    ))
}

#[derive(Deserialize)]
struct AssistantReviewRequest {
    path: String,
    question: String,
    expected_hash: String,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    graph_revision: Option<String>,
    #[serde(default)]
    scope: ReviewScope,
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReviewScope {
    #[default]
    SelectedDocument,
    ActiveContext,
}

impl ReviewScope {
    fn name(self) -> &'static str {
        match self {
            Self::SelectedDocument => "selected_document",
            Self::ActiveContext => "active_context",
        }
    }
}

struct EvidenceSelection {
    evidence: Vec<super::assistant::GroundingEvidence>,
    complete: bool,
    candidate_files: usize,
    evidence_files: usize,
    omitted_files: Vec<String>,
    read_failures: Vec<String>,
    partition_failures: Vec<String>,
    excluded_test_ranges: usize,
    excluded_test_lines: usize,
    graph_revision: String,
}

#[derive(Deserialize)]
struct EvidencePreviewRequest {
    path: String,
    expected_hash: String,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    graph_revision: Option<String>,
    #[serde(default)]
    scope: ReviewScope,
}

async fn assistant_evidence_preview_handler(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    Json(body): Json<EvidencePreviewRequest>,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;
    let _workspace_guard = server.write_lock.lock().await;
    let _assistant_guard = server.assistant_lock.lock().await;
    let context = server.context_summary().map_err(internal_error)?;
    validate_requested_mode(body.mode.as_deref(), &context)?;
    let selected = server
        .ide
        .read_document(&body.path)
        .map_err(document_read_error)?;
    validate_expected_document_hash(&body.expected_hash, &selected.hash)?;
    let evidence_server = server.clone();
    let evidence_context = context.clone();
    let scope = body.scope;
    let selection = tokio::task::spawn_blocking(move || {
        select_review_evidence(&evidence_server, &evidence_context, selected, scope)
    })
    .await
    .map_err(internal_error)?
    .map_err(document_read_error)?;
    validate_requested_revision(body.graph_revision.as_deref(), &selection.graph_revision)?;
    revalidate_document_hash(&server, &body.path, &body.expected_hash)?;
    let evidence_chars = selection
        .evidence
        .iter()
        .map(|item| item.excerpt.len())
        .sum::<usize>();
    let manifest = selection
        .evidence
        .iter()
        .map(|item| {
            json!({
                "id": item.id,
                "path": item.path,
                "start_line": item.start_line,
                "end_line": item.end_line,
                "content_hash": item.content_hash,
                "excerpt_chars": item.excerpt.len()
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "scope": body.scope.name(),
        "selected_document_hash": body.expected_hash,
        "graph_revision": selection.graph_revision,
        "indexed_mode": context.mode,
        "indexed_tokens": context.estimated_tokens,
        "context_length": server.context_length,
        "candidate_files": selection.candidate_files,
        "evidence_files": selection.evidence_files,
        "evidence_chunks": selection.evidence.len(),
        "evidence_chars": evidence_chars,
        "evidence_char_budget": server.evidence_char_budget,
        "evidence_complete": selection.complete,
        "omitted_files": selection.omitted_files,
        "read_failures": selection.read_failures,
        "partition_failures": selection.partition_failures,
        "excluded_test_ranges": selection.excluded_test_ranges,
        "excluded_test_lines": selection.excluded_test_lines,
        "manifest": manifest
    })))
}

async fn assistant_review_handler(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    Json(body): Json<AssistantReviewRequest>,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;
    let _workspace_guard = server.write_lock.lock().await;
    let _assistant_guard = server.assistant_lock.lock().await;
    let context = server.context_summary().map_err(internal_error)?;
    validate_requested_mode(body.mode.as_deref(), &context)?;
    let selected = server
        .ide
        .read_document(&body.path)
        .map_err(document_read_error)?;
    validate_expected_document_hash(&body.expected_hash, &selected.hash)?;
    let evidence_server = server.clone();
    let evidence_context = context.clone();
    let scope = body.scope;
    let selection = tokio::task::spawn_blocking(move || {
        select_review_evidence(&evidence_server, &evidence_context, selected, scope)
    })
    .await
    .map_err(internal_error)?
    .map_err(document_read_error)?;
    validate_requested_revision(body.graph_revision.as_deref(), &selection.graph_revision)?;
    if selection.evidence.is_empty() {
        return Err(bad_request(
            "active context produced no review evidence; select Full or Full Extended",
        ));
    }
    revalidate_document_hash(&server, &body.path, &body.expected_hash)?;
    let review = server
        .assistant
        .review(&body.question, selection.evidence, selection.complete)
        .await
        .map_err(internal_error)?;
    Ok(Json(json!({
        "review": review,
        "context": {
            "requested_mode": body.mode,
            "selected_document_hash": body.expected_hash,
            "graph_revision": selection.graph_revision,
            "indexed_mode": context.mode,
            "indexed_tokens": context.estimated_tokens,
            "included_nodes": context.included.len(),
            "prompt_scope": body.scope.name(),
            "candidate_files": selection.candidate_files,
            "evidence_files": selection.evidence_files,
            "omitted_files": selection.omitted_files,
            "read_failures": selection.read_failures,
            "partition_failures": selection.partition_failures,
            "excluded_test_ranges": selection.excluded_test_ranges,
            "excluded_test_lines": selection.excluded_test_lines,
            "evidence_char_budget": server.evidence_char_budget,
            "evidence_complete": selection.complete
        }
    })))
}

fn validate_requested_mode(
    requested: Option<&str>,
    context: &super::ContextSummary,
) -> ApiResult<()> {
    if let Some(requested) = requested {
        if requested != context.mode.name() {
            return Err(conflict(format!(
                "context changed: requested {requested}, current {}",
                context.mode.name()
            )));
        }
    }
    Ok(())
}

fn validate_requested_revision(requested: Option<&str>, current: &str) -> ApiResult<()> {
    if let Some(requested) = requested {
        if requested != current {
            return Err(conflict(format!(
                "graph changed: requested {requested}, current {current}"
            )));
        }
    }
    Ok(())
}

fn validate_expected_document_hash(expected: &str, current: &str) -> ApiResult<()> {
    if expected != current {
        return Err(conflict(format!(
            "document changed: requested hash {expected}, current {current}"
        )));
    }
    Ok(())
}

fn revalidate_document_hash(server: &EvolveServer, path: &str, expected: &str) -> ApiResult<()> {
    let current = server.ide.read_document(path).map_err(|error| {
        conflict(format!(
            "document changed after evidence selection: expected hash {expected}, re-read failed: {error}"
        ))
    })?;
    validate_expected_document_hash(expected, &current.hash)
}

fn select_review_evidence(
    server: &EvolveServer,
    context: &super::ContextSummary,
    selected: DocumentSnapshot,
    scope: ReviewScope,
) -> Result<EvidenceSelection> {
    let graph = server.graph_snapshot()?;
    let selection_revision = graph_revision(&graph)?;
    if matches!(scope, ReviewScope::SelectedDocument) {
        let (mut evidence, complete) =
            evidence_from_document(&selected.path, &selected.content, &selected.hash, 800);
        renumber_evidence(&mut evidence);
        return Ok(EvidenceSelection {
            evidence,
            complete,
            candidate_files: 1,
            evidence_files: 1,
            omitted_files: Vec::new(),
            read_failures: Vec::new(),
            partition_failures: Vec::new(),
            excluded_test_ranges: 0,
            excluded_test_lines: 0,
            graph_revision: selection_revision,
        });
    }

    let included: HashSet<&str> = context.included.iter().map(String::as_str).collect();
    let selected_node = graph.nodes.iter().find(|node| {
        node.path
            .as_deref()
            .and_then(|path| server.ide.graph_logical_path(path).ok())
            .is_some_and(|path| path == selected.path)
    });
    let selected_node = selected_node.ok_or_else(|| {
        anyhow::anyhow!(
            "active-context review rejected: selected document {} is not indexed in the graph",
            selected.path
        )
    })?;
    if !included.contains(selected_node.id.as_str()) {
        anyhow::bail!(
            "active-context review rejected: selected document {} is excluded by {} mode",
            selected.path,
            context.mode.name()
        );
    }

    let mut priority_ids = Vec::new();
    priority_ids.push(selected_node.id.clone());
    let mut neighbors = graph
        .edges
        .iter()
        .filter_map(|edge| {
            if edge.from == selected_node.id {
                Some(edge.to.clone())
            } else if edge.to == selected_node.id {
                Some(edge.from.clone())
            } else {
                None
            }
        })
        .filter(|id| included.contains(id.as_str()))
        .collect::<Vec<_>>();
    neighbors.sort();
    neighbors.dedup();
    priority_ids.extend(neighbors);
    let mut remaining = context.included.clone();
    remaining.sort();
    priority_ids.extend(remaining);
    priority_ids.dedup();

    let mut documents = vec![selected];
    let mut logical_paths: HashSet<String> = documents
        .iter()
        .map(|document| document.path.clone())
        .collect();
    let mut read_failures = Vec::new();
    for id in priority_ids {
        let Some(path) = graph
            .nodes
            .iter()
            .find(|node| node.id == id)
            .and_then(|node| node.path.as_deref())
        else {
            continue;
        };
        match server.ide.read_graph_document(path) {
            Ok(document) if logical_paths.insert(document.path.clone()) => documents.push(document),
            Ok(_) => {}
            Err(error) => read_failures.push(format!("{id}: {error}")),
        }
    }

    let candidate_files = documents.len() + read_failures.len();
    let mut evidence = Vec::new();
    let mut evidence_files = 0usize;
    let mut used_chars = 0usize;
    let mut omitted_files = Vec::new();
    let mut complete = read_failures.is_empty();
    let mut partition_failures = Vec::new();
    let mut excluded_test_ranges = 0usize;
    let mut excluded_test_lines = 0usize;

    for document in documents {
        let excluded_ranges =
            if matches!(context.mode, ContextMode::Full) && document.language == "rust" {
                match server.ast.cfg_test_ranges(&document.content) {
                    Ok(ranges) => ranges,
                    Err(error) => {
                        complete = false;
                        omitted_files.push(document.path.clone());
                        partition_failures.push(format!("{}: {error}", document.path));
                        continue;
                    }
                }
            } else {
                Vec::new()
            };
        excluded_test_ranges += excluded_ranges.len();
        let (chunks, document_complete, document_excluded_lines) =
            evidence_from_document_excluding_ranges(
                &document.path,
                &document.content,
                &document.hash,
                usize::MAX,
                &excluded_ranges,
            );
        excluded_test_lines += document_excluded_lines;
        let chunk_count = chunks.len();
        let mut included_chunks = 0usize;
        for chunk in chunks {
            let chunk_chars = chunk.excerpt.len();
            if used_chars.saturating_add(chunk_chars) > server.evidence_char_budget {
                complete = false;
                break;
            }
            used_chars += chunk_chars;
            included_chunks += 1;
            evidence.push(chunk);
        }
        if included_chunks > 0 {
            evidence_files += 1;
        }
        if !document_complete || included_chunks < chunk_count {
            complete = false;
            omitted_files.push(document.path);
        }
    }
    omitted_files.sort();
    omitted_files.dedup();
    renumber_evidence(&mut evidence);

    Ok(EvidenceSelection {
        evidence,
        complete,
        candidate_files,
        evidence_files,
        omitted_files,
        read_failures,
        partition_failures,
        excluded_test_ranges,
        excluded_test_lines,
        graph_revision: selection_revision,
    })
}

fn renumber_evidence(evidence: &mut [super::assistant::GroundingEvidence]) {
    for (index, item) in evidence.iter_mut().enumerate() {
        item.id = format!("E{}", index + 1);
    }
}

async fn git_status_handler(State(server): State<Arc<EvolveServer>>) -> ApiResult<Json<Value>> {
    Ok(Json(
        serde_json::to_value(server.git.status().map_err(internal_error)?)
            .map_err(internal_error)?,
    ))
}

#[derive(Deserialize)]
struct GitBranchRequest {
    name: String,
    expected_head: String,
    #[serde(default)]
    confirm: bool,
}

async fn git_branch_handler(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    Json(body): Json<GitBranchRequest>,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;
    let _workspace_guard = server.write_lock.lock().await;
    let _git_guard = server.git_lock.lock().await;
    let result = server
        .git
        .create_branch(&body.name, &body.expected_head, body.confirm)
        .map_err(bad_action_error)?;
    Ok(Json(serde_json::to_value(result).map_err(internal_error)?))
}

async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        "content-security-policy",
        HeaderValue::from_static(CONTENT_SECURITY_POLICY),
    );
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    headers.insert("cache-control", HeaderValue::from_static("no-store"));
    response
}

fn required_path(params: &HashMap<String, String>) -> ApiResult<&str> {
    params
        .get("path")
        .map(String::as_str)
        .ok_or_else(|| bad_request("missing path query parameter"))
}

fn require_session(headers: &HeaderMap, server: &EvolveServer) -> ApiResult<()> {
    let supplied = headers
        .get(SESSION_HEADER)
        .and_then(|value| value.to_str().ok());
    if supplied != Some(server.session_token()) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "missing or invalid evolve session token" })),
        ));
    }
    Ok(())
}

fn graph_revision(graph: &Graph) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(graph)?)))
}

fn merge_derived_with_ontology(mut derived: Graph, persisted: Graph) -> Graph {
    let mut ids: HashSet<String> = derived.nodes.iter().map(|node| node.id.clone()).collect();
    let persisted_node_ids: HashSet<String> = persisted
        .nodes
        .iter()
        .filter(|node| matches!(node.layer, NodeLayer::Concept | NodeLayer::Preset))
        .map(|node| node.id.clone())
        .collect();
    for node in persisted
        .nodes
        .into_iter()
        .filter(|node| matches!(node.layer, NodeLayer::Concept | NodeLayer::Preset))
    {
        if ids.insert(node.id.clone()) {
            derived.nodes.push(node);
        }
    }
    derived.nodes.sort_by(|left, right| left.id.cmp(&right.id));

    let valid_ids: HashSet<&str> = derived.nodes.iter().map(|node| node.id.as_str()).collect();
    for edge in persisted.edges.into_iter().filter(|edge| {
        valid_ids.contains(edge.from.as_str())
            && valid_ids.contains(edge.to.as_str())
            && (persisted_node_ids.contains(&edge.from) || persisted_node_ids.contains(&edge.to))
    }) {
        let duplicate = derived.edges.iter().any(|current| {
            current.from == edge.from
                && current.to == edge.to
                && current.edge_type == edge.edge_type
        });
        if !duplicate {
            derived.edges.push(edge);
        }
    }
    derived
}

fn bad_request(message: impl Into<String>) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": message.into() })),
    )
}

fn conflict(message: impl Into<String>) -> ApiError {
    (
        StatusCode::CONFLICT,
        Json(json!({ "error": message.into() })),
    )
}

fn not_found(message: impl Into<String>) -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": message.into() })),
    )
}

fn internal_error(error: impl std::fmt::Display) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": error.to_string() })),
    )
}

fn bad_action_error(error: impl std::fmt::Display) -> ApiError {
    bad_request(error.to_string())
}

fn document_read_error(error: impl std::fmt::Display) -> ApiError {
    let message = error.to_string();
    if message.contains("does not exist") {
        not_found(message)
    } else if message.contains("rejected")
        || message.contains("allowed root")
        || message.contains("regular file")
        || message.contains("read limit")
        || message.contains("supported repository source set")
    {
        bad_request(message)
    } else {
        internal_error(message)
    }
}

fn document_write_error(error: impl std::fmt::Display) -> ApiError {
    let message = error.to_string();
    if message.contains("stale write") {
        (StatusCode::CONFLICT, Json(json!({ "error": message })))
    } else if message.contains("rejected")
        || message.contains("allowed root")
        || message.contains("regular file")
        || message.contains("supported repository source set")
    {
        bad_request(message)
    } else {
        internal_error(message)
    }
}
