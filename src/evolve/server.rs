//! Local HTTP workspace for the self-evolve graph, IDE, and grounded actions.

use anyhow::{Context, Result};
use axum::{
    extract::{Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
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
    ReviewProtocolError,
};
use super::context_fit::{fit_tier, FitBudget, FitOutcome, RequestedMode, TierMeasurer};
use super::deletion::preview_deletion;
use super::diagnostics::{AnalysisKind, DiagnosticsEngine};
use super::envelope::{build_envelope_with_root, ContextEnvelope, ProjectedDocument};
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
    #[cfg(feature = "self-improvement")]
    apply_runs: super::ApplyRegistry,
    ontology: Option<Arc<OntologyStore>>,
    project_root: Arc<PathBuf>,
    configured_model: Arc<String>,
    endpoint_host: Arc<String>,
    context_length: usize,
    requested_mode: Arc<RwLock<RequestedMode>>,
    fit_budget: FitBudget,
    last_fit: Arc<RwLock<Option<FitOutcome>>>,
    envelope: Arc<RwLock<Option<ContextEnvelope>>>,
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
        let requested = RequestedMode::parse(&config.context_mode).map_err(anyhow::Error::msg)?;
        if !(0.1..=1.0).contains(&config.context_fit_ratio) {
            anyhow::bail!(
                "context_fit_ratio must be within 0.1..=1.0, got {}",
                config.context_fit_ratio
            );
        }
        let fit_budget = FitBudget::new(
            config.context_length,
            config.max_tokens,
            config.context_fit_ratio,
        );
        let mut composer = ContextComposer::new(graph.clone());
        let (initial_mode, initial_fit) = fit_mode(&requested, &graph, &project_root, &fit_budget);
        composer.set_mode(initial_mode);
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

        let server = Self {
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
            #[cfg(feature = "self-improvement")]
            apply_runs: super::apply::new_registry(),
            ontology: persist_ontology.then(|| Arc::new(ontology)),
            project_root: Arc::new(project_root),
            configured_model: Arc::new(config.model.clone()),
            endpoint_host: Arc::new(endpoint_host),
            context_length: config.context_length,
            requested_mode: Arc::new(RwLock::new(requested)),
            fit_budget,
            last_fit: Arc::new(RwLock::new(initial_fit)),
            envelope: Arc::new(RwLock::new(None)),
            evidence_char_budget,
            session_token: Arc::new(uuid::Uuid::new_v4().to_string()),
            write_lock: Arc::new(AsyncMutex::new(())),
            analysis_lock: Arc::new(AsyncMutex::new(())),
            assistant_lock: Arc::new(AsyncMutex::new(())),
            git_lock: Arc::new(AsyncMutex::new(())),
        };
        server.rebuild_envelope()?;
        Ok(server)
    }

    pub fn session_token(&self) -> &str {
        self.session_token.as_str()
    }

    /// The apply-run registry. Exposed so tests can stage runs directly
    /// (spawning the agent subprocess is not test-viable); production callers
    /// go through the HTTP endpoints.
    #[cfg(feature = "self-improvement")]
    pub fn apply_registry(&self) -> super::ApplyRegistry {
        self.apply_runs.clone()
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
        let router = Router::new()
            .route("/api/workspace", get(workspace_handler))
            .route("/api/graph", get(graph_handler))
            .route("/api/context", get(context_handler))
            .route("/api/context/mode", post(context_mode_handler))
            .route("/api/context/custom", post(context_custom_handler))
            .route("/api/context/sizes", get(context_sizes_handler))
            .route("/api/context/select", get(context_select_handler))
            .route(
                "/api/context/select/symbols",
                get(context_select_symbols_handler),
            )
            .route("/api/context/map", get(context_map_handler))
            .route("/api/context/expand", get(context_expand_handler))
            .route("/api/context/trust", get(context_trust_handler))
            .route("/api/context/reduce", get(context_reduce_handler))
            .route("/api/context/dedup", get(context_dedup_handler))
            .route("/api/structure", get(structure_handler))
            .route(
                "/api/analysis/duplicate-functions",
                get(duplicate_fns_handler),
            )
            .route("/api/analysis/dead-code", get(dead_code_handler))
            .route("/api/graph/findings", get(graph_findings_handler))
            .route("/api/graph/clustered", get(graph_clustered_handler))
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
            .route("/api/assistant/task", post(assistant_task_handler))
            .route(
                "/api/assistant/orientation",
                get(assistant_orientation_handler),
            )
            .route("/api/graph/logical", get(graph_logical_handler))
            .route("/api/evolve/presets", get(evolve_presets_handler))
            .route("/api/graph/modules", get(graph_modules_handler))
            .route("/api/evolve/pairs", get(evolve_pairs_handler))
            .route(
                "/api/evolve/pairs/suggest",
                post(evolve_pair_suggest_handler),
            )
            .route("/api/git/status", get(git_status_handler))
            .route("/api/git/branch", post(git_branch_handler));
        // Apply endpoints exist only with the self-improvement feature: apply
        // staging rides on shadow worktrees from `evolution::ast_tools`, so
        // without the feature the routes (and the whole apply module) are
        // compiled out and no-default-features builds stay green.
        #[cfg(feature = "self-improvement")]
        let router = router
            .route("/api/actions/apply", post(apply_action_handler))
            .route("/api/actions/apply/commit", post(apply_commit_handler))
            .route("/api/actions/apply/status", get(apply_status_handler));
        with_web_fallback(router, Path::new(WEB_DIR))
            .with_state(Arc::new(self.clone()))
            .layer(middleware::from_fn(security_headers))
    }

    pub async fn start(&self, port: u16) -> Result<()> {
        // Graceful shutdown: `main` owns the SIGINT/SIGTERM handlers and flips
        // the process-global shutdown latch (`crate::shutdown_requested`); a
        // direct Ctrl-C covers embeddings that run the server without `main`.
        // Without this, the server served until `main`'s grace timer expired
        // and force-exited the process with code 1 — every "graceful" Evolve
        // shutdown was a forced exit 1.
        self.start_with_shutdown(port, async {
            tokio::select! {
                _ = crate::shutdown_requested() => {}
                _ = tokio::signal::ctrl_c() => {}
            }
        })
        .await
    }

    /// Serve until `shutdown` resolves, then return `Ok(())` so the caller
    /// exits 0. Split from `start` so tests can drive the shutdown branch
    /// without touching process-global signals or the shutdown latch.
    pub async fn start_with_shutdown(
        &self,
        port: u16,
        shutdown: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> Result<()> {
        let address = format!("127.0.0.1:{port}");
        let listener = TcpListener::bind(&address).await?;
        println!("Evolve workspace listening on http://{address}");
        axum::serve(listener, self.router())
            .with_graceful_shutdown(shutdown)
            .await?;
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

    fn requested_mode(&self) -> Result<RequestedMode> {
        self.requested_mode
            .read()
            .map(|requested| requested.clone())
            .map_err(|_| anyhow::anyhow!("context lock poisoned"))
    }

    /// Record the latest auto-fit outcome (`None` for a pinned tier) so the
    /// workspace UI can render the budget bar.
    fn record_fit(&self, outcome: Option<FitOutcome>) -> Result<()> {
        *self
            .last_fit
            .write()
            .map_err(|_| anyhow::anyhow!("context lock poisoned"))? = outcome;
        Ok(())
    }

    fn last_fit(&self) -> Result<Option<FitOutcome>> {
        self.last_fit
            .read()
            .map(|fit| fit.clone())
            .map_err(|_| anyhow::anyhow!("context lock poisoned"))
    }

    /// Clone of the cached ContextEnvelope (`None` until the first rebuild).
    fn cached_envelope(&self) -> Result<Option<ContextEnvelope>> {
        self.envelope
            .read()
            .map(|envelope| envelope.clone())
            .map_err(|_| anyhow::anyhow!("context lock poisoned"))
    }

    /// Rebuild the cached ContextEnvelope for the composer's current mode and
    /// included set. Returns the fresh envelope (also stored on the server).
    fn rebuild_envelope(&self) -> Result<ContextEnvelope> {
        let graph = self.graph_snapshot()?;
        let revision = graph_revision(&graph)?;
        let (mode, included) = {
            let composer = self
                .composer
                .read()
                .map_err(|_| anyhow::anyhow!("context lock poisoned"))?;
            (composer.mode().clone(), composer.included_nodes())
        };
        let root = self.project_root.as_ref().clone();
        let read_root = root.clone();
        let envelope =
            build_envelope_with_root(&graph, &mode, &included, &revision, &root, move |rel| {
                std::fs::read_to_string(read_root.join(rel)).ok()
            });
        *self
            .envelope
            .write()
            .map_err(|_| anyhow::anyhow!("context lock poisoned"))? = Some(envelope.clone());
        Ok(envelope)
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
        let requested = self.requested_mode()?;
        let mut current = self
            .composer
            .write()
            .map_err(|_| anyhow::anyhow!("context lock poisoned"))?;
        // A hand-picked custom selection must survive refreshes (the composer
        // is rebuilt from scratch on every save): carry it over before the
        // composer is replaced.
        let custom_selection = if matches!(requested, RequestedMode::Fixed(ContextMode::Custom)) {
            Some(current.included_nodes())
        } else {
            None
        };
        let mut composer = ContextComposer::new(refreshed.clone());
        let (mode, outcome) =
            fit_mode(&requested, &refreshed, &self.project_root, &self.fit_budget);
        if let Some(ids) = custom_selection {
            composer.set_custom(ids);
        } else {
            composer.set_mode(mode);
        }
        *current = composer;
        drop(current);
        self.record_fit(outcome)?;
        self.rebuild_envelope()?;
        self.save_graph()?;
        Ok(revision)
    }
}

/// Resolve a requested mode against the current graph. `Auto` measures the
/// tier ladder and logs the decision — a warning when even Map overflows.
/// Returns the resolved mode plus the fit outcome (`Some` for `Auto`, `None`
/// for a pinned tier) so callers can surface the budget in the UI.
fn fit_mode(
    requested: &RequestedMode,
    graph: &Graph,
    root: &Path,
    budget: &FitBudget,
) -> (ContextMode, Option<FitOutcome>) {
    match requested {
        RequestedMode::Fixed(mode) => (mode.clone(), None),
        RequestedMode::Auto => {
            let outcome: FitOutcome = fit_tier(&TierMeasurer::new(graph, root), budget);
            if outcome.fits {
                tracing::info!(
                    "auto context tier: {} ({} <= {} tokens)",
                    outcome.mode.name(),
                    outcome.measured_tokens,
                    outcome.budget_tokens
                );
            } else {
                tracing::warn!(
                    "auto context tier: even map tier ({} tokens) exceeds the {}-token \
                     budget; selecting map and relying on the overflow backstop",
                    outcome.measured_tokens,
                    outcome.budget_tokens
                );
            }
            (outcome.mode.clone(), Some(outcome))
        }
    }
}

/// Budget gate for pinned (non-auto) tiers: build the envelope the candidate
/// selection would produce on a throwaway composer and reject with a typed
/// 422 when it overflows the usable budget. The live composer is untouched,
/// so a rejected pin leaves the server's context state unchanged.
fn reject_over_budget(
    server: &EvolveServer,
    apply: impl FnOnce(&mut ContextComposer),
) -> ApiResult<()> {
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let revision = graph_revision(&graph).map_err(internal_error)?;
    let mut composer = ContextComposer::new(graph.clone());
    apply(&mut composer);
    let root = server.project_root.as_ref().clone();
    let read_root = root.clone();
    let candidate = build_envelope_with_root(
        &graph,
        composer.mode(),
        &composer.included_nodes(),
        &revision,
        &root,
        move |rel| std::fs::read_to_string(read_root.join(rel)).ok(),
    );
    let budget = server.fit_budget.usable();
    if candidate.total_tokens > budget {
        return Err(unprocessable(json!({
            "error": "context_over_budget",
            "mode": candidate.mode.name(),
            "measured_tokens": candidate.total_tokens,
            "budget_tokens": budget,
        })));
    }
    Ok(())
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
    let fit = server.last_fit().map_err(internal_error)?;
    let envelope = server.cached_envelope().map_err(internal_error)?;
    let context_value = context_json(
        &context,
        server.context_length,
        &server.requested_mode().map_err(internal_error)?,
        fit.as_ref(),
        envelope.as_ref(),
    )
    .map_err(internal_error)?;
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
    let fit = server.last_fit().map_err(internal_error)?;
    let envelope = server.cached_envelope().map_err(internal_error)?;
    let mut value = context_json(
        &summary,
        server.context_length,
        &server.requested_mode().map_err(internal_error)?,
        fit.as_ref(),
        envelope.as_ref(),
    )
    .map_err(internal_error)?;
    // The Map tier's cost is a compiled artifact, not a per-node sum, so the
    // composer reports 0. Report the real measured map size instead.
    if matches!(summary.mode, ContextMode::Map) {
        let graph = server.graph_snapshot().map_err(internal_error)?;
        let root = server.project_root.as_ref().clone();
        let map_tokens =
            tokio::task::spawn_blocking(move || super::build_map(&graph, &root).map_tokens)
                .await
                .map_err(join_error)?;
        if let Some(obj) = value.as_object_mut() {
            obj.insert("estimated_tokens".into(), json!(map_tokens));
            obj.insert(
                "fits_context_window".into(),
                json!(map_tokens <= server.context_length),
            );
            obj.insert(
                "overflow_tokens".into(),
                json!(map_tokens.saturating_sub(server.context_length)),
            );
            if let Some(prod) = obj.get_mut("production").and_then(|p| p.as_object_mut()) {
                prod.insert("tokens".into(), json!(map_tokens));
            }
        }
    }
    Ok(Json(value))
}

#[derive(Deserialize)]
struct ContextModeRequest {
    mode: String,
    preset: Option<String>,
}

#[derive(Deserialize)]
struct ContextCustomRequest {
    components: Vec<String>,
}

/// Function-level duplicate/near-duplicate clones across the source tree —
/// the "excess code" that whole-file dedup misses. Read-only.
async fn duplicate_fns_handler(State(server): State<Arc<EvolveServer>>) -> ApiResult<Json<Value>> {
    let root = server.project_root.join("src");
    let pairs = tokio::task::spawn_blocking(move || super::FnDedupAnalyzer::new(root).find())
        .await
        .map_err(join_error)?
        .map_err(internal_error)?;
    let exact = pairs.iter().filter(|p| p.kind == "exact").count();
    Ok(Json(json!({
        "total": pairs.len(),
        "exact": exact,
        "near": pairs.len() - exact,
        "pairs": pairs,
    })))
}

/// The code graph aggregated into the 10 architectural clusters (loop-role
/// super-nodes) with inter-cluster dependency edges — same shape as /api/graph,
/// so the D3 view can default to 10 nodes and expand on demand.
async fn graph_clustered_handler(
    State(server): State<Arc<EvolveServer>>,
) -> ApiResult<Json<Value>> {
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let clustered = super::clustered(&graph);
    Ok(Json(json!({
        "nodes": clustered.nodes,
        "edges": clustered.edges,
        "cluster_count": clustered.nodes.len(),
    })))
}

/// Per-node graph findings with the actions each enables — dead symbols
/// (`promote_to_hotpath` / `stage_deletion`) and duplicate functions
/// (`merge_duplicate`), keyed by file path so the graph can overlay them.
async fn graph_findings_handler(State(server): State<Arc<EvolveServer>>) -> ApiResult<Json<Value>> {
    let root = server.project_root.as_ref().clone();
    let src = root.join("src");
    let (dead, dupes) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let dead = super::DeadCodeAnalyzer::new(&root).find()?;
        let dupes = super::FnDedupAnalyzer::new(&src).find()?;
        Ok((dead, dupes))
    })
    .await
    .map_err(join_error)?
    .map_err(internal_error)?;

    // Accumulate per file path.
    use std::collections::BTreeMap;
    let mut nodes: BTreeMap<String, (Vec<Value>, Vec<Value>)> = BTreeMap::new();
    for d in &dead {
        if d.cfg_gated {
            continue; // platform/feature code, not a hot-path candidate
        }
        nodes.entry(d.path.clone()).or_default().0.push(json!({
            "name": d.name, "kind": d.kind, "line": d.line, "is_pub": d.is_pub,
        }));
    }
    for p in &dupes {
        let entry = json!({
            "kind": p.kind, "similarity": p.similarity,
            "first": p.first, "second": p.second,
        });
        nodes
            .entry(p.first.path.clone())
            .or_default()
            .1
            .push(entry.clone());
        if p.second.path != p.first.path {
            nodes
                .entry(p.second.path.clone())
                .or_default()
                .1
                .push(entry);
        }
    }

    let entries: Vec<Value> = nodes
        .into_iter()
        .map(|(path, (dead_syms, dup_fns))| {
            let mut actions = Vec::new();
            if !dead_syms.is_empty() {
                actions.push("promote_to_hotpath");
                actions.push("stage_deletion");
            }
            if !dup_fns.is_empty() {
                actions.push("merge_duplicate");
            }
            json!({
                "path": path,
                "dead_symbols": dead_syms,
                "duplicate_functions": dup_fns,
                "actions": actions,
            })
        })
        .collect();

    Ok(Json(json!({
        "nodes_with_findings": entries.len(),
        "dead_symbols_total": dead.iter().filter(|d| !d.cfg_gated).count(),
        "duplicate_functions_total": dupes.len(),
        "nodes": entries,
    })))
}

#[cfg(feature = "self-improvement")]
#[derive(serde::Deserialize)]
struct ApplyActionRequest {
    kind: String,
    #[serde(default)]
    target: String,
    #[serde(default)]
    prompt: Option<String>,
}

/// Build the agent task prompt for a kind/target, or use a caller override.
#[cfg(feature = "self-improvement")]
fn apply_prompt(kind: &str, target: &str, custom: Option<&str>) -> String {
    if let Some(p) = custom.filter(|s| !s.trim().is_empty()) {
        return p.to_string();
    }
    let scope = if target.is_empty() {
        "the codebase".to_string()
    } else {
        format!("`{target}`")
    };
    match kind {
        "consolidate" => format!(
            "Find duplicated functions in {scope} and consolidate them: extract the shared logic \
             into a helper that both call sites use, preserving behavior exactly. Then run \
             `cargo build` and `cargo test` and make sure they pass."
        ),
        "cleanup" => format!(
            "Remove unused/dead public functions in {scope} that are referenced nowhere. After \
             removing, run `cargo build --all-targets` to confirm nothing breaks and revert any \
             removal that does."
        ),
        "refactor" => format!(
            "Refactor {scope} to improve clarity while preserving behavior. Keep `cargo build` \
             and `cargo test` green."
        ),
        "extend" => format!(
            "Extend {scope} with the requested capability, add tests, and keep the build green."
        ),
        _ => format!("Work on {scope}, keeping the build and tests green."),
    }
}

/// Apply an evolve action by driving the agent (`selfware run --yolo`) as a
/// subprocess. Requires a clean working tree so the resulting diff is exactly
/// the agent's work. POST /api/actions/apply {kind, target, prompt?}.
#[cfg(feature = "self-improvement")]
async fn apply_action_handler(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    Json(body): Json<ApplyActionRequest>,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;
    let status = server.git.status().map_err(internal_error)?;
    // `.worktrees/` holds this feature's own staged shadows, and `.selfware/`
    // is the server's own state directory (evolve-graph.yaml is written at
    // startup) — neither is user dirt. Without the .selfware exemption, Apply
    // blocks itself on a clean project with a misleading "uncommitted" error.
    let blocking: Vec<_> = status
        .files
        .iter()
        .filter(|f| !f.path.starts_with(".worktrees/") && !f.path.starts_with(".selfware/"))
        .collect();
    if !blocking.is_empty() {
        return Err(bad_request(format!(
            "working tree has {} uncommitted path(s); commit or stash first so the agent's diff \
             stays isolated and reviewable",
            blocking.len()
        )));
    }
    let prompt = apply_prompt(&body.kind, &body.target, body.prompt.as_deref());
    let root = server.project_root.as_ref().clone();
    let id = super::apply::spawn(prompt.clone(), root, server.apply_runs.clone())
        .await
        .map_err(internal_error)?;
    Ok(Json(
        json!({ "id": id, "status": "running", "prompt": prompt }),
    ))
}

/// Poll an apply run's status + streamed output. GET /api/actions/apply/status?id=X
#[cfg(feature = "self-improvement")]
async fn apply_status_handler(
    State(server): State<Arc<EvolveServer>>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let id = params
        .get("id")
        .ok_or_else(|| bad_request("missing id parameter"))?;
    let run = super::apply::get(&server.apply_runs, id)
        .await
        .ok_or_else(|| not_found(format!("unknown apply run: {id}")))?;
    Ok(Json(serde_json::to_value(run).map_err(internal_error)?))
}

#[cfg(feature = "self-improvement")]
#[derive(serde::Deserialize)]
struct ApplyCommitRequest {
    run_id: String,
    diff_digest: String,
}

/// One-use merge of a staged apply run into the live checkout.
/// POST /api/actions/apply/commit {run_id, diff_digest} →
/// {merged: true, new_head, files_changed}. Typed errors per the isolation
/// spec §3: 404 `unknown_run` (bad/used id or wrong digest), 409 `base_moved`
/// (live HEAD changed since staging), 409 `not_staged` (run exists but is not
/// Staged — 409 because it is a state conflict, the same class as base_moved),
/// 500 for infra failures.
#[cfg(feature = "self-improvement")]
async fn apply_commit_handler(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    Json(body): Json<ApplyCommitRequest>,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;
    let root = server.project_root.as_ref().clone();
    let outcome =
        super::apply::commit_staged(&server.apply_runs, &body.run_id, &body.diff_digest, &root)
            .await
            .map_err(commit_error)?;
    Ok(Json(json!({
        "merged": true,
        "new_head": outcome.new_head,
        "files_changed": outcome.files_changed,
    })))
}

/// Map a commit-step failure onto the isolation spec §3 taxonomy.
#[cfg(feature = "self-improvement")]
fn commit_error(error: super::apply::CommitError) -> ApiError {
    use super::apply::CommitError;
    match &error {
        CommitError::UnknownRun(_) => not_found(error.to_string()),
        CommitError::NotStaged(_) | CommitError::BaseMoved { .. } => conflict(error.to_string()),
        CommitError::Git(_) => internal_error(error),
    }
}

/// Reachability-based dead code: symbols whose name appears only at their own
/// definition (text-based, so cfg-gated cross-platform code isn't false-flagged).
async fn dead_code_handler(State(server): State<Arc<EvolveServer>>) -> ApiResult<Json<Value>> {
    let root = server.project_root.as_ref().clone();
    let dead = tokio::task::spawn_blocking(move || super::DeadCodeAnalyzer::new(root).find())
        .await
        .map_err(join_error)?
        .map_err(internal_error)?;
    let confident = dead.iter().filter(|d| !d.cfg_gated).count();
    Ok(Json(json!({
        "total": dead.len(),
        "confident": confident,
        "cfg_gated": dead.len() - confident,
        "symbols": dead,
    })))
}

/// Structural outline for navigation: every file's classes (struct/enum/trait)
/// with the methods in their impl blocks, plus free functions. Read-only.
async fn structure_handler(State(server): State<Arc<EvolveServer>>) -> ApiResult<Json<Value>> {
    let root = server.project_root.join("src");
    let files = tokio::task::spawn_blocking(move || super::StructureAnalyzer::new(root).outline())
        .await
        .map_err(join_error)?
        .map_err(internal_error)?;
    // Group by top-level component for the navigator.
    use std::collections::BTreeMap;
    let mut by_component: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let (mut class_total, mut method_total) = (0usize, 0usize);
    for f in &files {
        class_total += f.classes.len();
        method_total +=
            f.classes.iter().map(|c| c.methods.len()).sum::<usize>() + f.free_functions.len();
        let component = f
            .path
            .trim_start_matches("src/")
            .split('/')
            .next()
            .unwrap_or("")
            .replace(".rs", "");
        by_component
            .entry(component)
            .or_default()
            .push(serde_json::to_value(f).unwrap_or(Value::Null));
    }
    let components: Vec<Value> = by_component
        .into_iter()
        .map(|(name, files)| json!({ "component": name, "files": files }))
        .collect();
    Ok(Json(json!({
        "components": components,
        "class_count": class_total,
        "symbol_count": method_total,
    })))
}

/// Task-aware context selection: given a task kind (extend/refactor/consolidate/
/// cleanup/understand) and a target, return the source files that task needs and
/// the role each plays. `/api/context/select?kind=refactor&target=agent::execution`
async fn context_select_handler(
    State(server): State<Arc<EvolveServer>>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let kind = super::TaskKind::parse(
        params
            .get("kind")
            .map(String::as_str)
            .unwrap_or("understand"),
    )
    .map_err(|e| bad_request(e.to_string()))?;
    let target = params.get("target").cloned().unwrap_or_default();
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let root = server.project_root.as_ref().clone();
    let selection =
        tokio::task::spawn_blocking(move || super::select_context(kind, &target, &graph, &root))
            .await
            .map_err(join_error)?
            .map_err(internal_error)?;
    Ok(Json(json!({
        "task_kind": selection.task_kind,
        "target": selection.target,
        "rationale": selection.rationale,
        "file_count": selection.files.len(),
        "files": selection.files,
    })))
}

/// Cap on symbol matches returned by `/api/context/select/symbols` — the same
/// "bounded answer, more on request" discipline as the map's symbol cap.
const MAX_SYMBOL_MATCHES: usize = 50;

/// Task-aware SYMBOL selection: the file-level `select_context` pass, then for
/// each selected Rust file the skeleton symbols whose name or signature
/// mentions the target (case-insensitive). Exact name matches sort first.
/// `GET /api/context/select/symbols?kind=..&target=..`.
async fn context_select_symbols_handler(
    State(server): State<Arc<EvolveServer>>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let kind = super::TaskKind::parse(
        params
            .get("kind")
            .map(String::as_str)
            .unwrap_or("understand"),
    )
    .map_err(|e| bad_request(e.to_string()))?;
    let target = params.get("target").cloned().unwrap_or_default();
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let root = server.project_root.as_ref().clone();
    let report =
        tokio::task::spawn_blocking(move || symbol_selection_json(kind, &target, &graph, &root))
            .await
            .map_err(join_error)?
            .map_err(internal_error)?;
    Ok(Json(report))
}

/// The task's matching symbols as a JSON report: exact name matches first,
/// then substring matches, capped at [`MAX_SYMBOL_MATCHES`].
fn symbol_selection_json(
    kind: super::TaskKind,
    target: &str,
    graph: &Graph,
    root: &Path,
) -> Result<Value> {
    let selection = super::select_context(kind, target, graph, root)?;
    let needle = target.to_lowercase();
    // File path → graph node id, for honest component attribution.
    let component_of: HashMap<&str, &str> = graph
        .nodes
        .iter()
        .filter_map(|n| n.path.as_deref().map(|p| (p, n.id.as_str())))
        .collect();
    let mut files_scanned = 0usize;
    let mut exact: Vec<Value> = Vec::new();
    let mut substring: Vec<Value> = Vec::new();
    for file in &selection.files {
        if !file.path.ends_with(".rs") {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(root.join(&file.path)) else {
            continue;
        };
        files_scanned += 1;
        let component = component_of
            .get(file.path.as_str())
            .copied()
            .unwrap_or(file.path.as_str());
        let skeleton = super::extract_rust_skeleton(Path::new(&file.path), &src);
        for item in &skeleton.items {
            let Some((name, item_kind, line, signature)) = symbol_entry(item) else {
                continue;
            };
            if !needle.is_empty()
                && !name.to_lowercase().contains(&needle)
                && !signature.to_lowercase().contains(&needle)
            {
                continue;
            }
            let entry = json!({
                "component": component,
                "path": file.path,
                "symbol": name,
                "kind": item_kind,
                "line": line,
                "signature": signature,
            });
            if name.eq_ignore_ascii_case(target) {
                exact.push(entry);
            } else {
                substring.push(entry);
            }
        }
    }
    let mut symbols = exact;
    symbols.extend(substring);
    symbols.truncate(MAX_SYMBOL_MATCHES);
    Ok(json!({
        "kind": format!("{kind:?}").to_lowercase(),
        "target": target,
        "files_scanned": files_scanned,
        "symbols": symbols,
    }))
}

/// Flatten a skeleton item into `(name, kind, line, signature)` for symbol
/// search results. `use` items have no single name to search by and are
/// skipped.
fn symbol_entry(item: &super::SkeletonItem) -> Option<(String, &'static str, usize, String)> {
    use super::SkeletonItem as S;
    Some(match item {
        S::Function {
            name,
            signature,
            line,
        } => (name.clone(), "function", *line, signature.clone()),
        S::Struct { name, line, .. } => (name.clone(), "struct", *line, format!("struct {name}")),
        S::Enum { name, line, .. } => (name.clone(), "enum", *line, format!("enum {name}")),
        S::Trait { name, line, .. } => (name.clone(), "trait", *line, format!("trait {name}")),
        S::Impl { target, line, .. } => (target.clone(), "impl", *line, format!("impl {target}")),
        S::Module { name, line } => (name.clone(), "module", *line, format!("mod {name}")),
        S::Const {
            name,
            type_hint,
            line,
        } => (
            name.clone(),
            "const",
            *line,
            format!("const {name}: {type_hint}"),
        ),
        S::Use { .. } => return None,
    })
}

/// Node count and token size of every selectable context mode, so the picker
/// can show the cost of each option. Read-only; does not change the active mode.
async fn context_sizes_handler(State(server): State<Arc<EvolveServer>>) -> ApiResult<Json<Value>> {
    let mut sizes = {
        let composer = server
            .composer
            .read()
            .map_err(|_| internal_error(anyhow::anyhow!("context lock poisoned")))?;
        composer.mode_sizes()
    };
    // The Map tier is a compiled index, not a per-node projection, so its cost is
    // measured from the rendered artifact and prepended as the smallest option.
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let root = server.project_root.as_ref().clone();
    let map = tokio::task::spawn_blocking(move || super::build_map(&graph, &root))
        .await
        .map_err(join_error)?;
    sizes.insert(
        0,
        super::ContextModeSize {
            mode: "map".to_string(),
            nodes: map.components,
            tokens: map.map_tokens,
        },
    );
    Ok(Json(json!({
        "sizes": sizes,
        "context_length": server.context_length,
    })))
}

/// The full context-reduction pipeline for a task: select the files a task needs,
/// strip comments + inline tests, then elide duplicate function bodies across
/// them — reporting tokens saved at each stage. `GET /api/context/dedup?kind=..&target=..`.
async fn context_dedup_handler(
    State(server): State<Arc<EvolveServer>>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let kind = super::TaskKind::parse(
        params
            .get("kind")
            .map(String::as_str)
            .unwrap_or("understand"),
    )
    .map_err(|e| bad_request(e.to_string()))?;
    let target = params.get("target").cloned().unwrap_or_default();
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let root = server.project_root.as_ref().clone();
    let ide = server.ide.clone();

    let report = tokio::task::spawn_blocking(move || -> anyhow::Result<Value> {
        let selection = super::select_context(kind, &target, &graph, &root)?;
        let tok = crate::token_count::estimate_content_tokens;
        let mut original = 0usize;
        let mut reduced_files = Vec::new();
        for file in selection.files.iter().take(20) {
            let Ok(doc) = ide.read_document(&file.path) else {
                continue;
            };
            original += tok(&doc.content);
            reduced_files.push((file.path.clone(), super::reduce_source(&doc.content)));
        }
        let after_reduce: usize = reduced_files.iter().map(|(_, c)| tok(c)).sum();
        let (deduped, elided) = super::dedup_context(&reduced_files);
        let after_dedup: usize = deduped.iter().map(|(_, c)| tok(c)).sum();
        Ok(json!({
            "files": reduced_files.len(),
            "original_tokens": original,
            "after_reduce_tokens": after_reduce,
            "after_dedup_tokens": after_dedup,
            "reduce_saved": original.saturating_sub(after_reduce),
            "dedup_saved": after_reduce.saturating_sub(after_dedup),
            "total_saved": original.saturating_sub(after_dedup),
            "functions_deduped": elided,
            "total_saved_pct": if original > 0 {
                ((original.saturating_sub(after_dedup) as f64 / original as f64) * 1000.0).round() / 10.0
            } else { 0.0 },
        }))
    })
    .await
    .map_err(join_error)?
    .map_err(internal_error)?;
    Ok(Json(report))
}

/// Reduce a source file to its losslessly-droppable core (comments + inline test
/// blocks removed) and report the token savings. `GET /api/context/reduce?path=..`.
/// Local, no model call.
async fn context_reduce_handler(
    State(server): State<Arc<EvolveServer>>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let path = required_path(&params)?;
    let document = server
        .ide
        .read_document(path)
        .map_err(document_read_error)?;
    let original = &document.content;
    let reduced = super::reduce_source(original);
    let original_tokens = crate::token_count::estimate_content_tokens(original);
    let reduced_tokens = crate::token_count::estimate_content_tokens(&reduced);
    let saved = original_tokens.saturating_sub(reduced_tokens);
    let pct = if original_tokens > 0 {
        (saved as f64 / original_tokens as f64) * 100.0
    } else {
        0.0
    };
    Ok(Json(json!({
        "path": path,
        "original_tokens": original_tokens,
        "reduced_tokens": reduced_tokens,
        "saved_tokens": saved,
        "saved_pct": (pct * 10.0).round() / 10.0,
        "reduced": reduced,
    })))
}

/// Assess one context source for safety: its provenance-based trust level and any
/// injection / pollution patterns (instruction-override, role-switch, hidden
/// unicode, instructions-in-data, exfiltration hints). Local, no model call.
/// `GET /api/context/trust?path=src/foo.rs[&source=workspace]`.
async fn context_trust_handler(
    State(server): State<Arc<EvolveServer>>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let path = required_path(&params)?;
    let document = server
        .ide
        .read_document(path)
        .map_err(document_read_error)?;
    // Classification comes from the graph node when known, else inferred by ext.
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let classification = graph
        .nodes
        .iter()
        .find(|n| n.path.as_deref() == Some(path))
        .map(|n| n.classification.clone())
        .unwrap_or_else(|| {
            if path.ends_with(".rs") {
                "rust_source".into()
            } else {
                "other".into()
            }
        });
    let source = match params.get("source").map(String::as_str) {
        Some("tool_output") => super::SourceKind::ToolOutput,
        Some("model_output") => super::SourceKind::ModelOutput,
        Some("external") => super::SourceKind::External,
        Some("memory") => super::SourceKind::Memory,
        Some("user") => super::SourceKind::User,
        _ => super::SourceKind::Workspace,
    };
    let report = super::analyze_source(path, source, &classification, &document.content);
    Ok(Json(serde_json::to_value(report).map_err(internal_error)?))
}

/// The component map — the smallest tier. Returns the rendered index plus the
/// compression it achieves over full source.
async fn context_map_handler(State(server): State<Arc<EvolveServer>>) -> ApiResult<Json<Value>> {
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let root = server.project_root.as_ref().clone();
    let map = tokio::task::spawn_blocking(move || super::build_map(&graph, &root))
        .await
        .map_err(join_error)?;
    let ratio = if map.map_tokens > 0 {
        map.full_tokens as f64 / map.map_tokens as f64
    } else {
        0.0
    };
    Ok(Json(json!({
        "components": map.components,
        "map_tokens": map.map_tokens,
        "full_tokens": map.full_tokens,
        "compression_ratio": ratio,
        "cards": map.cards,
        "rendered": map.rendered,
    })))
}

/// Expand one component from the map to real detail: interface signatures by
/// default, the full comment-stripped source with `?full=true`, or exactly one
/// symbol's numbered source span with `?symbol=<name>`.
async fn context_expand_handler(
    State(server): State<Arc<EvolveServer>>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let component = params
        .get("component")
        .cloned()
        .filter(|c| !c.is_empty())
        .ok_or_else(|| bad_request("expand requires a `component` query parameter"))?;
    let symbol = params.get("symbol").cloned().filter(|s| !s.is_empty());
    let full = params
        .get("full")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let root = server.project_root.as_ref().clone();
    let target = component.clone();
    let sym = symbol.clone();
    let content = tokio::task::spawn_blocking(move || {
        super::expand_component(&graph, &root, &target, sym.as_deref(), full)
    })
    .await
    .map_err(join_error)?
    .ok_or_else(|| match &symbol {
        Some(s) => bad_request(format!(
            "symbol `{s}` not found in component `{component}` (or the component itself is unknown)"
        )),
        None => bad_request(format!("unknown component: {component}")),
    })?;
    let mode = if symbol.is_some() {
        "symbol"
    } else if full {
        "full"
    } else {
        "signatures"
    };
    Ok(Json(json!({
        "component": component,
        "symbol": symbol,
        "mode": mode,
        "tokens": crate::token_count::estimate_content_tokens(&content),
        "content": content,
    })))
}

async fn context_mode_handler(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    Json(body): Json<ContextModeRequest>,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;
    let _assistant_guard = server.assistant_lock.lock().await;
    let requested = match body.mode.as_str() {
        "auto" => RequestedMode::Auto,
        "map" => RequestedMode::Fixed(ContextMode::Map),
        "lite" => RequestedMode::Fixed(ContextMode::Lite),
        "compact" | "skeleton" => RequestedMode::Fixed(ContextMode::Compact),
        "full" => RequestedMode::Fixed(ContextMode::Full),
        "full_extended" => RequestedMode::Fixed(ContextMode::FullExtended),
        "preset" => RequestedMode::Fixed(ContextMode::Preset(
            body.preset
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| bad_request("preset mode requires a preset name"))?,
        )),
        _ => return Err(bad_request("unknown context mode")),
    };
    let (mode, outcome) = match &requested {
        RequestedMode::Fixed(mode) => {
            reject_over_budget(&server, |composer| composer.set_mode(mode.clone()))?;
            (mode.clone(), None)
        }
        RequestedMode::Auto => {
            let graph = server.graph_snapshot().map_err(internal_error)?;
            fit_mode(&requested, &graph, &server.project_root, &server.fit_budget)
        }
    };
    *server
        .requested_mode
        .write()
        .map_err(|_| internal_error(anyhow::anyhow!("context lock poisoned")))? = requested.clone();
    server.record_fit(outcome).map_err(internal_error)?;
    let summary = {
        let mut composer = server
            .composer
            .write()
            .map_err(|_| internal_error(anyhow::anyhow!("context lock poisoned")))?;
        composer.set_mode(mode);
        composer.summary()
    };
    let envelope = server.rebuild_envelope().map_err(internal_error)?;
    let fit = server.last_fit().map_err(internal_error)?;
    Ok(Json(
        context_json(
            &summary,
            server.context_length,
            &requested,
            fit.as_ref(),
            Some(&envelope),
        )
        .map_err(internal_error)?,
    ))
}

/// Apply a hand-picked component selection as the active context (Custom mode).
/// An empty `components` list clears the custom selection and returns the
/// workspace to the auto-fitted tier.
async fn context_custom_handler(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    Json(body): Json<ContextCustomRequest>,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;
    let _assistant_guard = server.assistant_lock.lock().await;
    let requested = if body.components.is_empty() {
        RequestedMode::Auto
    } else {
        RequestedMode::Fixed(ContextMode::Custom)
    };
    if !body.components.is_empty() {
        let components = body.components.clone();
        reject_over_budget(&server, |composer| composer.set_custom(components))?;
    }
    *server
        .requested_mode
        .write()
        .map_err(|_| internal_error(anyhow::anyhow!("context lock poisoned")))? = requested.clone();
    let summary = if body.components.is_empty() {
        // Clearing: re-fit the tier ladder like the `auto` branch of the mode handler.
        let graph = server.graph_snapshot().map_err(internal_error)?;
        let (mode, outcome) =
            fit_mode(&requested, &graph, &server.project_root, &server.fit_budget);
        server.record_fit(outcome).map_err(internal_error)?;
        let mut composer = server
            .composer
            .write()
            .map_err(|_| internal_error(anyhow::anyhow!("context lock poisoned")))?;
        composer.set_mode(mode);
        composer.summary()
    } else {
        server.record_fit(None).map_err(internal_error)?;
        let mut composer = server
            .composer
            .write()
            .map_err(|_| internal_error(anyhow::anyhow!("context lock poisoned")))?;
        composer.set_custom(body.components);
        composer.summary()
    };
    let envelope = server.rebuild_envelope().map_err(internal_error)?;
    let fit = server.last_fit().map_err(internal_error)?;
    Ok(Json(
        context_json(
            &summary,
            server.context_length,
            &requested,
            fit.as_ref(),
            Some(&envelope),
        )
        .map_err(internal_error)?,
    ))
}

fn context_json(
    summary: &super::ContextSummary,
    context_length: usize,
    requested: &RequestedMode,
    auto_fit: Option<&FitOutcome>,
    envelope: Option<&ContextEnvelope>,
) -> Result<Value> {
    let mut value = serde_json::to_value(summary)?;
    if let Some(object) = value.as_object_mut() {
        object.insert("context_length".to_string(), json!(context_length));
        object.insert("requested_mode".to_string(), json!(requested.name()));
        let fit_value = match (requested, auto_fit) {
            (RequestedMode::Auto, Some(fit)) => json!({
                "measured_tokens": fit.measured_tokens,
                "budget_tokens": fit.budget_tokens,
                "fits": fit.fits,
            }),
            _ => Value::Null,
        };
        object.insert("auto_fit".to_string(), fit_value);
        let (env_tokens, env_hash) = match envelope {
            Some(env) => (json!(env.total_tokens), json!(env.content_hash)),
            None => (Value::Null, Value::Null),
        };
        object.insert("envelope_tokens".to_string(), env_tokens);
        object.insert("envelope_hash".to_string(), env_hash);
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
        .map_err(join_error)?;
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
    /// True only when the cached envelope passed the revision gate and
    /// actually backed the selected evidence; responses must report
    /// `content_hash: null` otherwise.
    envelope_authoritative: bool,
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
    // Only claim the envelope hash when the envelope actually backed the
    // evidence (revision gate passed); otherwise report null.
    let content_hash = if selection.envelope_authoritative {
        envelope_content_hash(&server)?
    } else {
        Value::Null
    };
    Ok(Json(json!({
        "scope": body.scope.name(),
        "selected_document_hash": body.expected_hash,
        "graph_revision": selection.graph_revision,
        "content_hash": content_hash,
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
    let envelope_authoritative = selection.envelope_authoritative;
    let review = server
        .assistant
        .review(&body.question, selection.evidence, selection.complete)
        .await
        .map_err(review_error)?;
    // Only claim the envelope hash when the envelope actually backed the
    // evidence (revision gate passed); otherwise report null.
    let content_hash = if envelope_authoritative {
        envelope_content_hash(&server)?
    } else {
        Value::Null
    };
    Ok(Json(json!({
        "review": review,
        "context": {
            "requested_mode": body.mode,
            "selected_document_hash": body.expected_hash,
            "graph_revision": selection.graph_revision,
            "content_hash": content_hash,
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

#[derive(serde::Deserialize)]
struct AssistantTaskRequest {
    kind: String,
    #[serde(default)]
    target: String,
    question: String,
    #[serde(default = "default_task_max_files")]
    max_files: usize,
    /// Strip comments from the selected source before building evidence — a
    /// ~18% token cut for smaller models. Line numbers are relative to the
    /// stripped code (self-consistent within the evidence).
    #[serde(default)]
    compact: bool,
    /// Prepend a non-citeable workspace orientation (architectural taxonomy +
    /// component map) so the model can place the cited evidence in the wider
    /// tree without loading every file. Defaults on.
    #[serde(default = "default_true")]
    orient: bool,
    /// When orienting, include the full component map (~28K tokens) alongside the
    /// always-present taxonomy. Turn off for the smallest windows. Defaults on.
    #[serde(default = "default_true")]
    include_map: bool,
}

fn default_true() -> bool {
    true
}

fn default_task_max_files() -> usize {
    8
}

/// Task-scoped grounded review: instead of the caller hand-picking one document,
/// the context_selector auto-selects the source a task kind needs (seed +
/// dependents / findings), evidence is built from those files, and the assistant
/// reviews against that. POST /api/assistant/task {kind, target, question}.
async fn assistant_task_handler(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    Json(body): Json<AssistantTaskRequest>,
) -> ApiResult<Json<Value>> {
    require_session(&headers, &server)?;
    let _assistant_guard = server.assistant_lock.lock().await;
    if body.question.trim().is_empty() {
        return Err(bad_request("task question cannot be empty"));
    }
    let kind = super::TaskKind::parse(&body.kind).map_err(|e| bad_request(e.to_string()))?;
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let root = server.project_root.as_ref().clone();
    let ide = server.ide.clone();
    let target = body.target.clone();
    let max_files = body.max_files.clamp(1, 20);
    let compact = body.compact;
    let orient = body.orient;
    let include_map = body.include_map;

    let (selected, mut evidence, complete, orientation) =
        tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            // Whole-tree navigation context (taxonomy + optional component map),
            // built once and shared as non-citeable background for the review.
            let orientation =
                orient.then(|| super::workspace_orientation(&graph, &root, include_map));
            let selection = super::select_context(kind, &target, &graph, &root)?;
            let mut files = selection.files;
            // Seeds first, then findings, then dependents/dependencies.
            files.sort_by_key(|file| match file.role.as_str() {
                "seed" => 0,
                "finding" => 1,
                "dependent" => 2,
                _ => 3,
            });
            let mut evidence = Vec::new();
            let mut complete = true;
            let mut used = Vec::new();
            for file in files.into_iter().take(max_files) {
                let Ok(doc) = ide.read_document(&file.path) else {
                    // A selected file that can't be read means the evidence is
                    // partial — never report complete coverage (AGENTS.md §3).
                    complete = false;
                    tracing::warn!(path = %file.path, "task evidence file unreadable; marking evidence incomplete");
                    continue;
                };
                // For smaller models, strip comments to cut the prompt (~18%).
                let content = if compact {
                    super::strip_comments(&doc.content)
                } else {
                    doc.content.clone()
                };
                let (ev, is_complete) = evidence_from_document(&doc.path, &content, &doc.hash, 120);
                complete &= is_complete;
                evidence.extend(ev);
                used.push(file);
            }
            Ok((used, evidence, complete, orientation))
        })
        .await
        .map_err(internal_error)?
        .map_err(internal_error)?;

    if evidence.is_empty() {
        return Err(bad_request(
            "task selection produced no readable source evidence",
        ));
    }
    renumber_evidence(&mut evidence);
    let orientation_tokens = orientation
        .as_deref()
        .map(crate::token_count::estimate_content_tokens)
        .unwrap_or(0);
    let review = server
        .assistant
        .review_with_orientation(&body.question, evidence, complete, orientation.as_deref())
        .await
        .map_err(review_error)?;
    // Task evidence ships fresh reads only; the cached envelope never backs
    // it, so the response must not claim the envelope's content hash.
    Ok(Json(json!({
        "task_kind": body.kind,
        "target": body.target,
        "content_hash": Value::Null,
        "selected_files": selected,
        "orientation": {
            "enabled": orientation.is_some(),
            "included_map": orientation.is_some() && include_map,
            "tokens": orientation_tokens,
        },
        "review": review,
    })))
}

/// Preview the exact non-citeable orientation (taxonomy + optional component map)
/// the assistant task flow injects — a local render, no model call. Use it to
/// audit what background the model sees. `?include_map=false` for taxonomy only.
async fn assistant_orientation_handler(
    State(server): State<Arc<EvolveServer>>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let include_map = params
        .get("include_map")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let root = server.project_root.as_ref().clone();
    let text = tokio::task::spawn_blocking(move || {
        super::workspace_orientation(&graph, &root, include_map)
    })
    .await
    .map_err(join_error)?;
    Ok(Json(json!({
        "included_map": include_map,
        "tokens": crate::token_count::estimate_content_tokens(&text),
        "text": text,
    })))
}

/// The self-improvement preset library — the directions selfware can expand in,
/// each with its task, invariants, context recipe, verification, and a ready
/// run-prompt with the guardrails baked in. Local, no model call.
async fn evolve_presets_handler(
    State(_server): State<Arc<EvolveServer>>,
) -> ApiResult<Json<Value>> {
    let presets = super::presets();
    let items: Vec<Value> = presets
        .iter()
        .map(|p| {
            let mut v = serde_json::to_value(p).unwrap_or(Value::Null);
            if let Some(obj) = v.as_object_mut() {
                obj.insert("prompt".into(), json!(super::render_preset_prompt(p)));
            }
            v
        })
        .collect();
    Ok(Json(json!({
        "preset_count": items.len(),
        "presets": items,
    })))
}

/// The logical layer — ~9 capabilities (what the system does) with their
/// invariants, derived modules, and dependency edges. The top of the
/// comprehension ladder, rendered in the D3 {nodes, edges} shape.
async fn graph_logical_handler(State(server): State<Arc<EvolveServer>>) -> ApiResult<Json<Value>> {
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let root = server.project_root.as_ref().clone();
    let model = tokio::task::spawn_blocking(move || super::build_logical_model(&graph, &root))
        .await
        .map_err(join_error)?;
    let nodes: Vec<Value> = model
        .capabilities
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "label": c.name,
                "layer": "Code",
                "purpose": c.purpose,
                "invariants": c.invariants,
                "clusters": c.clusters,
                "modules": c.modules,
                "depends_on": c.depends_on,
                "tokens": c.tokens,
                "module_count": c.modules.len(),
            })
        })
        .collect();
    let edges: Vec<Value> = model
        .edges
        .iter()
        .map(
            |e| json!({ "from": e.from, "to": e.to, "edge_type": "DependsOn", "weight": e.weight }),
        )
        .collect();
    Ok(Json(json!({
        "nodes": nodes,
        "edges": edges,
        "capability_count": model.capabilities.len(),
    })))
}

/// The crate's module graph, seeded from `src/lib.rs`: one node per declared
/// top-level module (with visibility, section, and cfg gate), metrics aggregated
/// from the file graph, and module-level DependsOn edges. The clean entry graph.
async fn graph_modules_handler(State(server): State<Arc<EvolveServer>>) -> ApiResult<Json<Value>> {
    let root = server.project_root.as_ref().clone();
    let manifest = super::parse_module_manifest(&root).map_err(internal_error)?;
    let graph = server.graph_snapshot().map_err(internal_error)?;

    // Aggregate file-graph metrics per top-level module.
    use std::collections::BTreeMap;
    let mut metrics: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    for node in graph
        .nodes
        .iter()
        .filter(|n| n.layer == super::NodeLayer::Code)
    {
        let comp = super::clusters::component_of(&node.id);
        let e = metrics.entry(comp).or_default();
        e.0 += node.tokens;
        e.1 += node.lines;
        e.2 += node.files.max(1);
    }

    let declared: std::collections::BTreeSet<&str> =
        manifest.modules.iter().map(|m| m.name.as_str()).collect();
    let reexport_count = |name: &str| {
        manifest
            .reexports
            .iter()
            .filter(|r| r.module == name)
            .count()
    };

    let nodes: Vec<Value> = manifest
        .modules
        .iter()
        .map(|m| {
            let (tokens, lines, files) = metrics.get(&m.name).copied().unwrap_or((0, 0, 0));
            json!({
                "id": m.name,
                "label": m.name,
                "layer": "Code",
                "path": super::module_path(&root, &m.name),
                "tokens": tokens,
                "lines": lines,
                "files": files,
                "visibility": m.visibility,
                "category": m.category,
                "cfg_feature": m.cfg_feature,
                "test_only": m.test_only,
                "classification": if m.test_only { "test" } else { "rust_source" },
                "cluster": super::clusters::cluster_of(&m.name),
                "reexports": reexport_count(&m.name),
            })
        })
        .collect();

    // Collapse file-graph DependsOn edges to module level, keeping only edges
    // between two declared modules.
    let mut weights: BTreeMap<(String, String), usize> = BTreeMap::new();
    for edge in &graph.edges {
        if !matches!(edge.edge_type, super::EdgeType::DependsOn) {
            continue;
        }
        let from = super::clusters::component_of(&edge.from);
        let to = super::clusters::component_of(&edge.to);
        if from == to || !declared.contains(from.as_str()) || !declared.contains(to.as_str()) {
            continue;
        }
        *weights.entry((from, to)).or_default() += 1;
    }
    let edges: Vec<Value> = weights
        .into_iter()
        .map(|((from, to), weight)| {
            json!({
                "from": from, "to": to, "edge_type": "DependsOn", "weight": weight,
            })
        })
        .collect();

    Ok(Json(json!({
        "nodes": nodes,
        "edges": edges,
        "module_count": manifest.modules.len(),
        "reexport_count": manifest.reexports.len(),
    })))
}

/// List every level-1 connected component pair in the production graph — the
/// candidates for combined-evolution suggestions. Local, no model call.
async fn evolve_pairs_handler(State(server): State<Arc<EvolveServer>>) -> ApiResult<Json<Value>> {
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let pairs = super::connected_pairs(&graph);
    Ok(Json(json!({
        "pair_count": pairs.len(),
        "cross_cluster": pairs.iter().filter(|p| p.cross_cluster).count(),
        "pairs": pairs,
    })))
}

#[derive(serde::Deserialize)]
struct PairSuggestRequest {
    a: String,
    b: String,
    /// Return the assembled pair context without calling the model — lets a
    /// caller inspect exactly what would be sent (and its token cost) first.
    #[serde(default)]
    dry_run: bool,
}

/// Suggest how a connected component pair should evolve together. Builds the
/// combined context (both components' public surface + relationship) and asks
/// the configured model. `dry_run` returns just the context for inspection.
async fn evolve_pair_suggest_handler(
    State(server): State<Arc<EvolveServer>>,
    headers: HeaderMap,
    Json(body): Json<PairSuggestRequest>,
) -> ApiResult<Json<Value>> {
    if body.a.trim().is_empty() || body.b.trim().is_empty() {
        return Err(bad_request("suggest requires components `a` and `b`"));
    }
    if body.a == body.b {
        return Err(bad_request("a and b must be different components"));
    }
    let graph = server.graph_snapshot().map_err(internal_error)?;
    let root = server.project_root.as_ref().clone();
    // Resolve the pair (order-independent) so its relationship/cluster metadata
    // comes from the real graph edges; require it to be actually connected.
    let (a, b) = (body.a.clone(), body.b.clone());
    let pair = super::connected_pairs(&graph)
        .into_iter()
        .find(|p| (p.a == a && p.b == b) || (p.a == b && p.b == a));
    let Some(pair) = pair else {
        return Err(bad_request(format!(
            "{a} and {b} are not level-1 connected in the production graph"
        )));
    };

    let context = {
        let graph = graph.clone();
        let root = root.clone();
        let pair = pair.clone();
        tokio::task::spawn_blocking(move || super::pair_context(&graph, &root, &pair))
            .await
            .map_err(join_error)?
    };
    let context_tokens = crate::token_count::estimate_content_tokens(&context);

    if body.dry_run {
        return Ok(Json(json!({
            "pair": pair,
            "context_tokens": context_tokens,
            "context": context,
            "dry_run": true,
        })));
    }

    require_session(&headers, &server)?;
    let _assistant_guard = server.assistant_lock.lock().await;
    let prompt = super::suggest_prompt(&context);
    let (text, model, usage) = server
        .assistant
        .freeform(super::SUGGEST_SYSTEM, &prompt)
        .await
        .map_err(internal_error)?;
    // Best-effort structured extraction — surface the parsed suggestions when the
    // model returned valid JSON, always keep the raw text.
    let suggestions = extract_json_object(&text);
    Ok(Json(json!({
        "pair": pair,
        "model": model,
        "usage": usage,
        "context_tokens": context_tokens,
        "suggestions": suggestions,
        "raw": text,
    })))
}

/// Pull the first balanced `{...}` JSON object out of model text and parse it,
/// tolerating prose or code fences around it. Returns null on failure.
fn extract_json_object(text: &str) -> Value {
    let bytes = text.as_bytes();
    let Some(start) = text.find('{') else {
        return Value::Null;
    };
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escaped = false;
    for i in start..bytes.len() {
        match bytes[i] {
            b'"' if !escaped => in_str = !in_str,
            b'\\' if in_str => {
                escaped = !escaped;
                continue;
            }
            b'{' if !in_str => depth += 1,
            b'}' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    return serde_json::from_str(&text[start..=i]).unwrap_or(Value::Null);
                }
            }
            _ => {}
        }
        escaped = false;
    }
    Value::Null
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
        // Full mode excludes inline cfg(test) blocks from the selected
        // document, exactly as the active-context path excludes them from
        // every shipped document; FullExtended (and the reduced tiers) keep
        // them. Without this the selected doc shipped its tests while the
        // neighborhood dropped them, and the review claimed complete evidence
        // for content the tier contract says is out of scope.
        let mut complete = true;
        let mut partition_failures = Vec::new();
        let excluded_ranges =
            if matches!(context.mode, ContextMode::Full) && selected.language == "rust" {
                match server.ast.cfg_test_ranges(&selected.content) {
                    Ok(ranges) => ranges,
                    Err(error) => {
                        // Ship the document anyway — it is the only evidence
                        // this scope has — but report the partition failure
                        // instead of claiming clean, excluded evidence.
                        complete = false;
                        partition_failures.push(format!("{}: {error}", selected.path));
                        Vec::new()
                    }
                }
            } else {
                Vec::new()
            };
        let excluded_test_ranges = excluded_ranges.len();
        let (mut evidence, chunk_complete, excluded_test_lines) =
            evidence_from_document_excluding_ranges(
                &selected.path,
                &selected.content,
                &selected.hash,
                800,
                &excluded_ranges,
            );
        renumber_evidence(&mut evidence);
        return Ok(EvidenceSelection {
            evidence,
            complete: complete && chunk_complete,
            candidate_files: 1,
            evidence_files: 1,
            omitted_files: Vec::new(),
            read_failures: Vec::new(),
            partition_failures,
            excluded_test_ranges,
            excluded_test_lines,
            graph_revision: selection_revision,
            // Selected-document scope ships only the fresh read; the envelope
            // backs nothing here.
            envelope_authoritative: false,
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
    // Tier-projected context: neighborhood documents ship the cached
    // envelope's projected content instead of a fresh full read, but only
    // when the envelope was built from this exact graph revision. The
    // reviewed document already leads `documents` (fresh full read, hash
    // validated by the caller); the `logical_paths` dedup drops its envelope
    // twin below.
    let envelope = server.cached_envelope()?;
    let envelope_authoritative = envelope
        .as_ref()
        .is_some_and(|envelope| envelope.graph_revision == selection_revision);
    let envelope_docs: HashMap<&str, &ProjectedDocument> = match envelope.as_ref() {
        Some(envelope) if envelope.graph_revision == selection_revision => envelope
            .documents
            .iter()
            .map(|doc| (doc.id.as_str(), doc))
            .collect(),
        Some(envelope) => {
            tracing::debug!(
                "evidence: cached envelope revision {} != current {}; using fresh reads",
                envelope.graph_revision,
                selection_revision
            );
            HashMap::new()
        }
        None => {
            tracing::debug!("evidence: no cached envelope; using fresh reads");
            HashMap::new()
        }
    };
    for id in priority_ids {
        let Some(path) = graph
            .nodes
            .iter()
            .find(|node| node.id == id)
            .and_then(|node| node.path.as_deref())
        else {
            continue;
        };
        if let Some(projected) = envelope_docs.get(id.as_str()) {
            // Normalize through the same logical-path mapping the fresh-read
            // branch gets from `read_graph_document`, so the selected
            // document's envelope twin dedups against it. If the envelope path
            // cannot be normalized, fall back to a fresh full read (same as a
            // per-doc envelope miss).
            match server.ide.graph_logical_path(&projected.path) {
                Ok(path) => {
                    let document = DocumentSnapshot {
                        language: language_for_projected(&path),
                        lines: projected.content.lines().count(),
                        hash: format!("{:x}", Sha256::digest(projected.content.as_bytes())),
                        content: projected.content.clone(),
                        path,
                    };
                    if logical_paths.insert(document.path.clone()) {
                        documents.push(document);
                    }
                    continue;
                }
                Err(error) => {
                    tracing::debug!(
                        "evidence: node {id} envelope path failed to normalize ({error}); using fresh read"
                    );
                }
            }
        } else if !envelope_docs.is_empty() {
            tracing::debug!("evidence: node {id} missing from cached envelope; using fresh read");
        }
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
        envelope_authoritative,
    })
}

fn renumber_evidence(evidence: &mut [super::assistant::GroundingEvidence]) {
    for (index, item) in evidence.iter_mut().enumerate() {
        item.id = format!("E{}", index + 1);
    }
}

/// Language tag for an envelope-projected document. Only `rust` is load
/// bearing downstream (Full-mode cfg(test) exclusion); everything else is
/// chunked without exclusions either way.
fn language_for_projected(path: &str) -> String {
    if path.ends_with(".rs") {
        "rust".to_string()
    } else {
        "plaintext".to_string()
    }
}

/// The cached envelope's content hash for response payloads (`null` when no
/// envelope has been built yet).
fn envelope_content_hash(server: &EvolveServer) -> ApiResult<Value> {
    Ok(match server.cached_envelope().map_err(internal_error)? {
        Some(envelope) => json!(envelope.content_hash),
        None => Value::Null,
    })
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

/// DNS-rebinding guard: this server hands out a session token on
/// /api/workspace. It binds 127.0.0.1, but a rebound attacker page would
/// arrive same-origin — reject any request whose Host header is not a
/// loopback name.
fn is_loopback_host(host: Option<&str>) -> bool {
    let Some(host) = host else {
        return true; // HTTP/1.0 clients may omit Host
    };
    let name = host.trim_matches(|c| c == '[' || c == ']');
    let name = name.split(':').next().unwrap_or("");
    matches!(name, "127.0.0.1" | "localhost" | "::1" | "")
}

async fn security_headers(request: Request, next: Next) -> Response {
    let host = request
        .headers()
        .get("host")
        .and_then(|value| value.to_str().ok());
    if !is_loopback_host(host) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "loopback host required (DNS-rebinding protection)"})),
        )
            .into_response();
    }
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

fn unprocessable(value: Value) -> ApiError {
    (StatusCode::UNPROCESSABLE_ENTITY, Json(value))
}

/// Map a grounded-review failure: typed protocol failures (spec §2.1) become
/// 422 with the spec body verbatim; anything else stays an untyped 500.
fn review_error(error: anyhow::Error) -> ApiError {
    match error.downcast_ref::<ReviewProtocolError>() {
        Some(protocol) => unprocessable(protocol.body()),
        None => internal_error(error),
    }
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

/// Map a spawn_blocking JoinError without leaking the panic payload — tokio's
/// Display includes the panic message, which is unreviewed internals and must
/// not reach API clients (AGENTS.md §3). The full error stays in the log.
fn join_error(error: tokio::task::JoinError) -> ApiError {
    tracing::error!("background task failed: {error}");
    internal_error("background task failed")
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

/// UI fallback for the evolve workspace. DEV OVERRIDE: when the source
/// checkout's web dir exists on disk, serve from it (`ServeDir`) so UI
/// iteration doesn't need rebuilds. Otherwise serve the assets embedded in
/// the binary — release artifacts ship only binary + docs, so `src/evolve/web`
/// doesn't exist at runtime there and a disk-only fallback 404s / and /app.js.
fn with_web_fallback<S>(router: Router<S>, web_dir: &Path) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    if web_dir.is_dir() {
        router.fallback_service(ServeDir::new(web_dir).append_index_html_on_directories(true))
    } else {
        router.fallback(embedded_web)
    }
}

/// The embedded asset map: path → (bytes, mime), compiled into the binary via
/// `include_str!`/`include_bytes!`. d3 and lucide are REQUIRED by index.html
/// for the core UI (graph + icons), so they're embedded; vendor/monaco (12 MB,
/// 114 files, editor-only) is NOT — editor mode requires a source checkout
/// (where the dev override serves it from disk); embedded-mode requests for
/// other vendor paths get a 404 with an honest note.
fn embedded_asset(path: &str) -> Option<(&'static [u8], &'static str)> {
    Some(match path {
        "/" | "/index.html" => (
            include_str!("web/index.html").as_bytes(),
            "text/html; charset=utf-8",
        ),
        "/app.js" => (
            include_str!("web/app.js").as_bytes(),
            "text/javascript; charset=utf-8",
        ),
        "/style.css" => (
            include_str!("web/style.css").as_bytes(),
            "text/css; charset=utf-8",
        ),
        "/editor.html" => (
            include_str!("web/editor.html").as_bytes(),
            "text/html; charset=utf-8",
        ),
        "/vendor/d3/d3.min.js" => (
            include_bytes!("web/vendor/d3/d3.min.js").as_slice(),
            "text/javascript; charset=utf-8",
        ),
        "/vendor/lucide/lucide.min.js" => (
            include_bytes!("web/vendor/lucide/lucide.min.js").as_slice(),
            "text/javascript; charset=utf-8",
        ),
        _ => return None,
    })
}

/// Fallback handler serving the embedded UI assets (release-binary mode).
async fn embedded_web(uri: axum::http::Uri) -> Response {
    match embedded_asset(uri.path()) {
        Some((body, mime)) => ([(axum::http::header::CONTENT_TYPE, mime)], body).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "not found (monaco editor assets are served only from a source checkout)",
        )
            .into_response(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/server_web_test.rs"]
mod server_web_test;
