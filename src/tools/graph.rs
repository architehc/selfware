//! Graph query tools over the cached evolve graph.
//!
//! Eight read-only tools expose `evolve::GraphIndex` to the agent loop:
//! - `GraphSummaryTool` (`graph_summary`) — fixed-cost L0 orientation
//! - `HotspotsTool` (`hotspots`) — top-K nodes by a size/complexity metric
//! - `ContextPackTool` (`context_pack`) — budget-greedy task context pack
//! - `ImpactTool` (`impact`) — reverse-DependsOn blast-radius closure
//! - `NeighborsTool` (`neighbors`) — typed edge list around one node
//! - `TestMapTool` (`test_map`) — tests owning/owned by one node
//! - `CyclesTool` (`cycles`) — DependsOn dependency cycles
//! - `DupsTool` (`dups`) — duplicate/similar code pairs
//!
//! Every response carries an honesty envelope: the graph revision, when the
//! graph file was built, the *measured* token cost of the payload (via
//! `token_count::estimate_content_tokens`, never byte÷4 estimates), the
//! budget the payload was packed against, and exactly what was dropped when
//! the budget forced truncation (AGENTS.md rules 3 and 4). All work runs in
//! `tokio::task::spawn_blocking` so a first-call YAML parse never stalls the
//! runtime.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::evolve::clusters::{component_of, taxonomy_outline};
use crate::evolve::expand_component;
use crate::evolve::graph_cache::{graph_path, shared_graph_index};
use crate::evolve::graph_index::{
    edge_type_name, lexical_hits, parse_edge_kind, split_terms, Direction, GraphIndex, Metric,
};
use crate::evolve::map::{component_cards, render_card, ComponentCard};
use crate::evolve::{EdgeType, Node, NodeLayer};

use super::Tool;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn layer_name(layer: NodeLayer) -> &'static str {
    match layer {
        NodeLayer::Code => "code",
        NodeLayer::Test => "test",
        NodeLayer::Structure => "structure",
        NodeLayer::Concept => "concept",
        NodeLayer::Preset => "preset",
        NodeLayer::Auxiliary => "auxiliary",
        NodeLayer::Symbol => "symbol",
    }
}

fn parse_layer(s: &str) -> Option<NodeLayer> {
    match s.trim().to_lowercase().as_str() {
        "code" => Some(NodeLayer::Code),
        "test" => Some(NodeLayer::Test),
        "structure" => Some(NodeLayer::Structure),
        "concept" => Some(NodeLayer::Concept),
        "preset" => Some(NodeLayer::Preset),
        "auxiliary" => Some(NodeLayer::Auxiliary),
        "symbol" => Some(NodeLayer::Symbol),
        _ => None,
    }
}

/// When the graph file under `root` was last built (its mtime), RFC 3339.
fn graph_built_at(root: &Path) -> String {
    std::fs::metadata(graph_path(root))
        .and_then(|m| m.modified())
        .map(DateTime::<Utc>::from)
        .map(|t| t.to_rfc3339())
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Wrap a tool payload in the honesty envelope. `measured_tokens` covers the
/// serialized payload section (the envelope itself is a small constant);
/// `budget` of `None` means the output is self-bounding and reports the
/// measured cost as its own budget.
fn respond(
    index: &GraphIndex,
    root: &Path,
    payload: Map<String, Value>,
    budget: Option<usize>,
    truncated: bool,
    dropped: Value,
) -> Value {
    let payload = Value::Object(payload);
    let measured = crate::token_count::estimate_content_tokens(&payload.to_string());
    let mut out = Map::new();
    out.insert("graph_revision".into(), json!(index.revision));
    out.insert(
        "graph_path".into(),
        json!(graph_path(root).display().to_string()),
    );
    out.insert("graph_built_at".into(), json!(graph_built_at(root)));
    out.insert("measured_tokens".into(), json!(measured));
    out.insert("budget_tokens".into(), json!(budget.unwrap_or(measured)));
    out.insert("truncated".into(), json!(truncated));
    out.insert("dropped".into(), dropped);
    out.insert("payload".into(), payload);
    Value::Object(out)
}

/// Run a blocking graph computation off the async hot path.
async fn run_blocking<F>(f: F) -> Result<Value>
where
    F: FnOnce() -> Result<Value> + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| anyhow!("graph tool task failed: {e}"))?
}

// ---------------------------------------------------------------------------
// graph_summary
// ---------------------------------------------------------------------------

const DEFAULT_SUMMARY_BUDGET: usize = 1500;
const SUMMARY_HOTSPOTS: usize = 20;

/// Fixed-cost architectural orientation: taxonomy outline, per-cluster
/// rollups, and the top hotspots by measured tokens.
pub struct GraphSummaryTool {
    root: PathBuf,
}

impl GraphSummaryTool {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl Tool for GraphSummaryTool {
    fn name(&self) -> &str {
        "graph_summary"
    }

    fn description(&self) -> &str {
        "Architectural orientation from the evolve graph: the 10-cluster taxonomy, top hotspots \
         by measured tokens, and per-cluster rollups (nodes, tokens, lines), greedily packed to \
         `budget`. Read-only; reports graph revision, build time, and measured output cost. \
         Requires a graph built by `selfware self-evolve`."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "budget": {
                    "type": "integer",
                    "description": "Token budget for the summary payload. Default 1500.",
                    "default": 1500,
                    "minimum": 1
                }
            },
            "required": [],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let budget = args
            .get("budget")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_SUMMARY_BUDGET as u64) as usize;
        let root = self.root.clone();
        run_blocking(move || {
            let index = shared_graph_index(&root)?;
            Ok(build_summary(&index, &root, budget))
        })
        .await
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::read_only()
    }
}

fn build_summary(index: &GraphIndex, root: &Path, budget: usize) -> Value {
    let render = |outline: &str, hotspots: &[Value], clusters: &[Value]| {
        json!({
            "outline": outline,
            "hotspots": hotspots,
            "clusters": clusters,
        })
        .to_string()
    };
    let packed = pack_summary_sections(index, budget, render);
    let mut payload = Map::new();
    payload.insert("outline".into(), json!(packed.outline));
    payload.insert("hotspots".into(), Value::Array(packed.hotspots));
    payload.insert("clusters".into(), Value::Array(packed.clusters));
    respond(
        index,
        root,
        payload,
        Some(budget),
        packed.truncated,
        Value::Object(packed.dropped),
    )
}

/// The sections of a packed graph summary (shared by the `graph_summary`
/// tool and the task-start L0 injection note).
struct PackedSummary {
    outline: String,
    hotspots: Vec<Value>,
    clusters: Vec<Value>,
    truncated: bool,
    dropped: Map<String, Value>,
}

/// Greedy packer behind both summary renderings: outline always ships, then
/// hotspot rows, then cluster rollups. The first row that would push the
/// *measured* rendered size over `budget` stops its section and everything
/// after it is reported as dropped. `render` decides the output shape (JSON
/// for the tool, compact text for the injection note) so both callers pack
/// against the same measured cost they actually ship.
fn pack_summary_sections(
    index: &GraphIndex,
    budget: usize,
    render: impl Fn(&str, &[Value], &[Value]) -> String,
) -> PackedSummary {
    let outline = taxonomy_outline(&index.graph);
    let hotspot_rows: Vec<Value> = index
        .hotspots(
            Metric::Tokens,
            Some(NodeLayer::Code),
            None,
            SUMMARY_HOTSPOTS,
        )
        .iter()
        .map(|node| {
            json!({
                "id": node.id,
                "layer": layer_name(node.layer),
                "tokens": node.tokens,
                "complexity": node.complexity,
            })
        })
        .collect();
    let cluster_rows: Vec<Value> = crate::evolve::clusters::clustered(&index.graph)
        .nodes
        .iter()
        .map(|cluster| {
            json!({
                "cluster": cluster.id,
                "components": cluster.member_count,
                "tokens": cluster.tokens,
                "lines": cluster.lines,
            })
        })
        .collect();

    let hotspot_rows_total = hotspot_rows.len();
    let cluster_rows_total = cluster_rows.len();
    let mut kept_hotspots: Vec<Value> = Vec::new();
    let mut kept_clusters: Vec<Value> = Vec::new();
    let mut truncated = false;
    for row in hotspot_rows {
        kept_hotspots.push(row);
        if crate::token_count::estimate_content_tokens(&render(
            &outline,
            &kept_hotspots,
            &kept_clusters,
        )) > budget
        {
            kept_hotspots.pop();
            truncated = true;
            break;
        }
    }
    if !truncated {
        for row in cluster_rows {
            kept_clusters.push(row);
            if crate::token_count::estimate_content_tokens(&render(
                &outline,
                &kept_hotspots,
                &kept_clusters,
            )) > budget
            {
                kept_clusters.pop();
                truncated = true;
                break;
            }
        }
    }
    let dropped_hotspots = hotspot_rows_total.saturating_sub(kept_hotspots.len());
    let dropped_clusters = cluster_rows_total.saturating_sub(kept_clusters.len());
    let mut dropped = Map::new();
    if dropped_hotspots > 0 {
        dropped.insert("hotspots".into(), json!(dropped_hotspots));
    }
    if dropped_clusters > 0 {
        dropped.insert("clusters".into(), json!(dropped_clusters));
    }
    PackedSummary {
        outline,
        hotspots: kept_hotspots,
        clusters: kept_clusters,
        truncated,
        dropped,
    }
}

/// Token budget for the task-start L0 orientation note's summary payload.
const L0_NOTE_BUDGET: usize = 1500;

/// Render the compact L0 graph orientation note injected at task start so
/// the model knows the evolve graph exists and what it says. `None` when
/// the graph is missing or unreadable — silent absence by design: the note
/// must never surface an error to the model, and the graph is never built
/// here (only `selfware self-evolve` builds it).
pub fn graph_summary_note(root: &Path) -> Option<String> {
    let index = shared_graph_index(root).ok()?;
    let text_render = |outline: &str, hotspots: &[Value], clusters: &[Value]| {
        let mut out = outline.to_string();
        if !hotspots.is_empty() {
            out.push_str("\n\nHotspots (tokens): ");
            out.push_str(
                &hotspots
                    .iter()
                    .map(|row| format!("{} ({})", row["id"].as_str().unwrap_or("?"), row["tokens"]))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        if !clusters.is_empty() {
            out.push_str("\nClusters: ");
            out.push_str(
                &clusters
                    .iter()
                    .map(|row| {
                        format!(
                            "{} {} tok",
                            row["cluster"].as_str().unwrap_or("?"),
                            row["tokens"]
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        out
    };
    let packed = pack_summary_sections(&index, L0_NOTE_BUDGET, text_render);
    let summary_text = text_render(&packed.outline, &packed.hotspots, &packed.clusters);
    let mut note = format!(
        "<selfware_context_note kind=graph_summary content_revision={} built_at={}>\n{}\n",
        index.revision,
        graph_built_at(root),
        summary_text
    );
    if packed.truncated {
        note.push_str(&format!(
            "(packed to the {}-token budget; dropped: {})\n",
            L0_NOTE_BUDGET,
            Value::Object(packed.dropped)
        ));
    }
    note.push_str(
        "Call these graph tools directly by name (no tool_search needed): graph_summary, hotspots, context_pack, impact, neighbors, test_map, cycles, dups.\n</selfware_context_note>",
    );
    Some(note)
}

// ---------------------------------------------------------------------------
// hotspots
// ---------------------------------------------------------------------------

const DEFAULT_HOTSPOT_K: usize = 20;

/// Top-K graph nodes by a size/complexity metric.
pub struct HotspotsTool {
    root: PathBuf,
}

impl HotspotsTool {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl Tool for HotspotsTool {
    fn name(&self) -> &str {
        "hotspots"
    }

    fn description(&self) -> &str {
        "Top-K evolve-graph nodes by `metric` (tokens, complexity, or density = complexity per \
         line, falling back to tokens per line), optionally restricted to one layer and \
         optionally excluding an id/path prefix (e.g. \"scratchpad\"). Rows carry id, path, \
         layer, tokens, lines, complexity, and warning_count. Read-only."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "metric": {
                    "type": "string",
                    "enum": ["tokens", "complexity", "density"],
                    "description": "Ranking metric. Default \"tokens\".",
                    "default": "tokens"
                },
                "layer": {
                    "type": "string",
                    "enum": ["code", "test", "structure", "concept", "preset", "auxiliary", "symbol"],
                    "description": "Restrict to one node layer. Omit for all file-level layers (symbol nodes are opt-in)."
                },
                "exclude_prefix": {
                    "type": "string",
                    "description": "Exclude nodes whose id or path starts with this prefix (e.g. \"scratchpad\"). Applied before ranking."
                },
                "k": {
                    "type": "integer",
                    "description": "Number of rows. Default 20.",
                    "default": 20,
                    "minimum": 1
                }
            },
            "required": [],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let metric = match args.get("metric").and_then(|v| v.as_str()) {
            Some(raw) => Metric::parse(raw).ok_or_else(|| {
                anyhow!("unknown metric '{raw}' (expected tokens|complexity|density)")
            })?,
            None => Metric::Tokens,
        };
        let layer = match args.get("layer").and_then(|v| v.as_str()) {
            Some(raw) => Some(parse_layer(raw).ok_or_else(|| {
                anyhow!(
                    "unknown layer '{raw}' (expected code|test|structure|concept|preset|auxiliary)"
                )
            })?),
            None => None,
        };
        let exclude_prefix = args
            .get("exclude_prefix")
            .and_then(|v| v.as_str())
            .map(String::from);
        let k = args
            .get("k")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_HOTSPOT_K as u64) as usize;
        let root = self.root.clone();
        run_blocking(move || {
            let index = shared_graph_index(&root)?;
            let rows: Vec<Value> = index
                .hotspots(metric, layer, exclude_prefix.as_deref(), k)
                .iter()
                .map(|node| {
                    json!({
                        "id": node.id,
                        "path": node.path,
                        "layer": layer_name(node.layer),
                        "tokens": node.tokens,
                        "lines": node.lines,
                        "complexity": node.complexity,
                        "warning_count": node.warning_count,
                    })
                })
                .collect();
            let mut payload = Map::new();
            payload.insert("metric".into(), json!(format!("{metric:?}").to_lowercase()));
            if let Some(prefix) = &exclude_prefix {
                payload.insert("exclude_prefix".into(), json!(prefix));
            }
            payload.insert("k".into(), json!(rows.len()));
            payload.insert("rows".into(), Value::Array(rows));
            // Self-bounding output (k rows): no budget enforced, so the
            // measured cost is reported as its own budget.
            Ok(respond(&index, &root, payload, None, false, json!({})))
        })
        .await
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::read_only()
    }
}

// ---------------------------------------------------------------------------
// context_pack
// ---------------------------------------------------------------------------

const DEFAULT_PACK_BUDGET: usize = 8000;
const SEED_COUNT: usize = 8;

/// Budget-greedy task context pack: lexical seed selection over the graph,
/// depth-1 dependent/dependency frontier, packed against a measured token
/// budget.
pub struct ContextPackTool {
    root: PathBuf,
}

impl ContextPackTool {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl Tool for ContextPackTool {
    fn name(&self) -> &str {
        "context_pack"
    }

    fn description(&self) -> &str {
        "Assemble a token-budgeted context pack for a task from the evolve graph: lexical seed \
         files matching `task_keywords`, plus their depth-1 dependents (blast radius) and \
         dependencies. Seeds ship at `detail` level (component cards or interface signatures), \
         the frontier as cards. Reports measured tokens per document and lists everything the \
         budget forced out. Read-only."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_keywords": {
                    "anyOf": [
                        {"type": "string"},
                        {"type": "array", "items": {"type": "string"}}
                    ],
                    "description": "Keywords describing the task (module names, concepts). Terms of 3+ alphanumeric characters are matched against node ids and paths."
                },
                "token_budget": {
                    "type": "integer",
                    "description": "Token budget for the assembled pack. Default 8000.",
                    "default": 8000,
                    "minimum": 1
                },
                "detail": {
                    "type": "string",
                    "enum": ["cards", "signatures"],
                    "description": "Detail level for seed documents. Default \"cards\".",
                    "default": "cards"
                },
                "include_tests": {
                    "type": "boolean",
                    "description": "Also include test files linked to the seeds. Default false.",
                    "default": false
                }
            },
            "required": ["task_keywords"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let keywords: Vec<String> = match args.get("task_keywords") {
            Some(Value::String(s)) => vec![s.clone()],
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            _ => Vec::new(),
        };
        let budget = args
            .get("token_budget")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_PACK_BUDGET as u64) as usize;
        let detail = args
            .get("detail")
            .and_then(|v| v.as_str())
            .unwrap_or("cards")
            .to_string();
        if detail != "cards" && detail != "signatures" {
            bail!("unknown detail '{detail}' (expected cards|signatures)");
        }
        let include_tests = args
            .get("include_tests")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let root = self.root.clone();
        run_blocking(move || build_pack(&root, &keywords, budget, &detail, include_tests)).await
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::read_only()
    }
}

/// One node selected for the pack, with the role it plays and why. Frontier
/// selections name the edge type and direction (relative to the seed) that
/// connects them — models read these to judge whether the link matters.
struct Selection {
    id: String,
    role: &'static str,
    reason: String,
    /// Edge type connecting a frontier node to its seed (`None` for seeds).
    edge_type: Option<EdgeType>,
    /// Edge direction relative to the seed: `"in"` (frontier → seed),
    /// `"out"` (seed → frontier); `None` for seeds.
    direction: Option<&'static str>,
}

fn build_pack(
    root: &Path,
    keywords: &[String],
    budget: usize,
    detail: &str,
    include_tests: bool,
) -> Result<Value> {
    let index = shared_graph_index(root)?;
    let terms = split_terms(&keywords.join(" "));
    if terms.is_empty() {
        bail!(
            "task_keywords produced no searchable terms (need words of 3+ alphanumeric characters)"
        );
    }

    let code_nodes: Vec<&crate::evolve::Node> = index
        .graph
        .nodes
        .iter()
        .filter(|n| n.layer == NodeLayer::Code)
        .collect();
    let max_tokens = code_nodes
        .iter()
        .map(|n| n.tokens)
        .max()
        .unwrap_or(0)
        .max(1) as f64;
    let max_complexity = code_nodes
        .iter()
        .filter_map(|n| n.complexity)
        .fold(0.0f64, f64::max)
        .max(1.0);

    // score = 3·lexical(node) + 2·lexical(component) + tokens/max + 0.5·complexity/max
    let score_of = |node: &crate::evolve::Node| -> f64 {
        let node_hits = lexical_hits(node, &terms) as f64;
        let component = component_of(&node.id).to_lowercase();
        let component_hits = terms
            .iter()
            .filter(|t| component.contains(t.as_str()))
            .count() as f64;
        3.0 * node_hits
            + 2.0 * component_hits
            + node.tokens as f64 / max_tokens
            + 0.5 * node.complexity.unwrap_or(0.0) / max_complexity
    };

    if !code_nodes.iter().any(|n| lexical_hits(n, &terms) > 0) {
        // Honest no-match: nothing lexical to anchor on, so a size-prior pick
        // would be arbitrary. Suggest the closest real node ids instead.
        let mut payload = Map::new();
        payload.insert("matches".into(), json!(0));
        payload.insert(
            "note".into(),
            json!("no graph nodes matched the task keywords"),
        );
        payload.insert(
            "suggestions".into(),
            json!(index.nearest_matches(&terms[0], 5)),
        );
        return Ok(respond(
            &index,
            root,
            payload,
            Some(budget),
            false,
            json!({}),
        ));
    }

    let mut scored: Vec<(&crate::evolve::Node, f64)> =
        code_nodes.iter().map(|n| (*n, score_of(n))).collect();
    scored.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.id.cmp(&right.0.id))
    });

    let mut selections: Vec<Selection> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (node, _) in scored.iter().take(SEED_COUNT) {
        seen.insert(node.id.clone());
        selections.push(Selection {
            id: node.id.clone(),
            role: "seed",
            reason: format!("lexical match for task keywords ({})", terms.join(", ")),
            edge_type: None,
            direction: None,
        });
    }
    let seed_ids: Vec<String> = selections.iter().map(|s| s.id.clone()).collect();
    for seed in &seed_ids {
        for dependent in index.dependents(seed) {
            if seen.insert(dependent.clone()) {
                selections.push(Selection {
                    id: dependent.clone(),
                    role: "dependent",
                    reason: format!(
                        "depends on {seed} via depends_on edge (incoming to seed) — blast radius"
                    ),
                    edge_type: Some(EdgeType::DependsOn),
                    direction: Some("in"),
                });
            }
        }
        for dependency in index.dependencies(seed) {
            if seen.insert(dependency.clone()) {
                selections.push(Selection {
                    id: dependency.clone(),
                    role: "dependency",
                    reason: format!(
                        "dependency of {seed} via depends_on edge (outgoing from seed)"
                    ),
                    edge_type: Some(EdgeType::DependsOn),
                    direction: Some("out"),
                });
            }
        }
        if include_tests {
            for test in index.tests_for(seed) {
                if seen.insert(test.clone()) {
                    selections.push(Selection {
                        id: test.clone(),
                        role: "test",
                        reason: format!("tests {seed} via contains edge (outgoing from seed)"),
                        edge_type: Some(EdgeType::Contains),
                        direction: Some("out"),
                    });
                }
            }
        }
    }

    // Component cards for every selected node up front (one source read per
    // node) — frontier documents and `cards`-detail seeds both render these.
    let want: BTreeSet<String> = selections.iter().map(|s| component_of(&s.id)).collect();
    let cards: HashMap<String, ComponentCard> = component_cards(&index.graph, root, &want)
        .into_iter()
        .map(|card| (card.component.clone(), card))
        .collect();

    // Projection with an explicit per-node failure reason — a dropped
    // document must say WHY (no path / unreadable file / no projection),
    // never a bare "no readable source projection".
    let content_for = |selection: &Selection| -> std::result::Result<String, String> {
        let node = index
            .node(&selection.id)
            .ok_or_else(|| "node missing from graph".to_string())?;
        let rel = node
            .path
            .clone()
            .ok_or_else(|| "node has no path in the graph".to_string())?;
        let signature_detail =
            selection.role == "seed" && detail == "signatures" || selection.role == "test";
        let source_state = || {
            if std::fs::metadata(root.join(&rel)).is_ok() {
                None
            } else {
                Some(format!("source file unreadable: {rel}"))
            }
        };
        if signature_detail {
            expand_component(&index.graph, root, &selection.id, None, false).ok_or_else(|| {
                source_state().unwrap_or_else(|| {
                    if selection.role == "test" {
                        format!("test node: no signature projection for {rel} (parse failed)")
                    } else {
                        format!(
                            "no signature projection for {rel} (parse failed or non-Rust source)"
                        )
                    }
                })
            })
        } else {
            cards.get(&selection.id).map(render_card).ok_or_else(|| {
                source_state().unwrap_or_else(|| format!("no component card for {rel}"))
            })
        }
    };

    // Greedy fill in selection order (seeds first). After each addition the
    // assembled documents array is measured; the first document that busts
    // the budget stops packing and everything remaining is reported dropped.
    let mut documents: Vec<Value> = Vec::new();
    let mut included: Vec<String> = Vec::new();
    let mut dropped_detail: Vec<Value> = Vec::new();
    let mut truncated = false;
    let mut cheapest_cost: Option<usize> = None;
    for selection in &selections {
        if truncated {
            dropped_detail.push(json!({
                "id": selection.id,
                "role": selection.role,
                "reason": "budget exhausted",
            }));
            continue;
        }
        let content = match content_for(selection) {
            Ok(content) => content,
            Err(why) => {
                dropped_detail.push(json!({
                    "id": selection.id,
                    "role": selection.role,
                    "reason": why,
                }));
                continue;
            }
        };
        let content_tokens = crate::token_count::estimate_content_tokens(&content);
        if selection.role == "seed" {
            cheapest_cost = Some(cheapest_cost.map_or(content_tokens, |c| c.min(content_tokens)));
        }
        let node_path = index
            .node(&selection.id)
            .and_then(|n| n.path.clone())
            .unwrap_or_default();
        let mut candidate = json!({
            "id": selection.id,
            "path": node_path,
            "role": selection.role,
            "reason": selection.reason,
            "content": content,
            "tokens": content_tokens,
        });
        if let (Some(edge_type), Some(direction)) = (&selection.edge_type, selection.direction) {
            candidate["edge_type"] = json!(edge_type_name(edge_type));
            candidate["direction"] = json!(direction);
        }
        let mut trial = documents.clone();
        trial.push(candidate.clone());
        if crate::token_count::estimate_content_tokens(&Value::Array(trial).to_string()) > budget {
            truncated = true;
            dropped_detail.push(json!({
                "id": selection.id,
                "role": selection.role,
                "reason": format!("over budget: needs ~{content_tokens} tokens"),
            }));
            continue;
        }
        documents.push(candidate);
        included.push(selection.id.clone());
    }

    let total_tokens: usize = documents
        .iter()
        .filter_map(|d| d.get("tokens").and_then(|t| t.as_u64()))
        .sum::<u64>() as usize;
    let content_hash = pack_hash(&index.revision, &included, &documents);

    // Envelope-level dropped counts by role; per-item detail stays in the payload.
    let mut dropped_counts: Map<String, Value> = Map::new();
    for entry in &dropped_detail {
        let role = entry
            .get("role")
            .and_then(|r| r.as_str())
            .unwrap_or("unknown");
        let count = dropped_counts
            .get(role)
            .and_then(|c| c.as_u64())
            .unwrap_or(0)
            + 1;
        dropped_counts.insert(role.to_string(), json!(count));
    }

    let mut payload = Map::new();
    let fits = !documents.is_empty();
    payload.insert("fits".into(), json!(fits));
    if !fits {
        // Nothing fit the budget — say so and name the cheapest seed's
        // measured cost instead of silently shipping an empty pack.
        payload.insert("cheapest_cost_tokens".into(), json!(cheapest_cost));
        payload.insert(
            "note".into(),
            json!("no selected document fits the token budget"),
        );
    }
    payload.insert("detail".into(), json!(detail));
    payload.insert("included".into(), json!(included));
    payload.insert("documents".into(), Value::Array(documents));
    payload.insert("total_tokens".into(), json!(total_tokens));
    payload.insert("content_hash".into(), json!(content_hash));
    payload.insert("dropped_detail".into(), Value::Array(dropped_detail));
    Ok(respond(
        &index,
        root,
        payload,
        Some(budget),
        truncated,
        Value::Object(dropped_counts),
    ))
}

/// Content-address the pack the same way `evolve::envelope` does:
/// length-prefixed fields so boundaries cannot collide.
fn pack_hash(revision: &str, included: &[String], documents: &[Value]) -> String {
    fn hash_field(hasher: &mut Sha256, bytes: &[u8]) {
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, revision.as_bytes());
    hash_field(&mut hasher, b"context_pack");
    for id in included {
        hash_field(&mut hasher, id.as_bytes());
    }
    for doc in documents {
        if let Some(content) = doc.get("content").and_then(|c| c.as_str()) {
            hash_field(&mut hasher, content.as_bytes());
        }
    }
    format!("{:x}", hasher.finalize())
}

// ---------------------------------------------------------------------------
// impact / neighbors / test_map — single-node queries
// ---------------------------------------------------------------------------

/// Typed unknown-id error with actionable suggestions — node ids are
/// `module::path` style and models guess them wrong often, so a bare
/// "unknown id" dead-ends the loop.
fn require_node<'g>(index: &'g GraphIndex, id: &str) -> Result<&'g crate::evolve::Node> {
    index.node(id).ok_or_else(|| {
        let suggestions = index.nearest_matches(id, 5);
        anyhow!(
            "unknown node id '{id}'. Closest matches: {}",
            suggestions.join(", ")
        )
    })
}

fn required_id(args: &Value) -> Result<String> {
    args.get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| anyhow!("missing required string argument 'id'"))
}

const DEFAULT_IMPACT_DEPTH: usize = 2;
const DEFAULT_IMPACT_K: usize = 50;

/// Reverse-DependsOn blast radius of one node.
pub struct ImpactTool {
    root: PathBuf,
}

impl ImpactTool {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl Tool for ImpactTool {
    fn name(&self) -> &str {
        "impact"
    }

    fn description(&self) -> &str {
        "Blast radius of a graph node: the reverse-DependsOn BFS closure (everything that \
         depends on `id`, transitively up to `depth` hops), ranked by depth then tokens. Each \
         row carries the connecting edge_type, the hop it was reached through (`via`), and its \
         token cost so you can budget the fallout. Page large closures with `offset` (the \
         response's `next_offset` is null when done). Read-only."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Graph node id (module path, e.g. \"crate::tools::graph\")."
                },
                "depth": {
                    "type": "integer",
                    "description": "Maximum hops of reverse-DependsOn traversal. Default 2.",
                    "default": 2,
                    "minimum": 1
                },
                "k": {
                    "type": "integer",
                    "description": "Maximum rows per page. Default 50.",
                    "default": 50,
                    "minimum": 1
                },
                "offset": {
                    "type": "integer",
                    "description": "Row offset for paging (use the previous response's next_offset). Default 0.",
                    "default": 0,
                    "minimum": 0
                }
            },
            "required": ["id"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let id = required_id(&args)?;
        let depth = args
            .get("depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_IMPACT_DEPTH as u64) as usize;
        let k = args
            .get("k")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_IMPACT_K as u64) as usize;
        let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let root = self.root.clone();
        run_blocking(move || {
            let index = shared_graph_index(&root)?;
            require_node(&index, &id)?;
            let mut closure = index.impact_frontier(&id, depth);
            // Rank by depth then token cost, so the caller sees the cheapest
            // fallout first within each hop ring.
            closure.sort_by(|left, right| {
                left.1.cmp(&right.1).then_with(|| {
                    let tokens_of = |row: &(String, usize, String)| {
                        index.node(&row.0).map(|n| n.tokens).unwrap_or(0)
                    };
                    tokens_of(right)
                        .cmp(&tokens_of(left))
                        .then_with(|| left.0.cmp(&right.0))
                })
            });
            let total_rows = closure.len();
            let page: Vec<_> = closure.into_iter().skip(offset).take(k).collect();
            let next_offset = if offset + page.len() < total_rows {
                Some(offset + page.len())
            } else {
                None
            };
            let rows: Vec<Value> = page
                .iter()
                .map(|(row_id, row_depth, via)| {
                    let node = index.node(row_id);
                    json!({
                        "id": row_id,
                        "path": node.and_then(|n| n.path.clone()),
                        "layer": node.map(|n| layer_name(n.layer)),
                        "depth": row_depth,
                        "tokens": node.map(|n| n.tokens).unwrap_or(0),
                        "edge_type": edge_type_name(&EdgeType::DependsOn),
                        "via": via,
                    })
                })
                .collect();
            let dropped_count = total_rows.saturating_sub(offset + rows.len());
            let mut dropped = Map::new();
            if dropped_count > 0 {
                dropped.insert("impact".into(), json!(dropped_count));
            }
            let mut payload = Map::new();
            payload.insert("id".into(), json!(id));
            payload.insert("depth".into(), json!(depth));
            payload.insert("total_rows".into(), json!(total_rows));
            payload.insert("offset".into(), json!(offset));
            payload.insert("next_offset".into(), json!(next_offset));
            payload.insert("count".into(), json!(rows.len()));
            payload.insert("rows".into(), Value::Array(rows));
            Ok(respond(
                &index,
                &root,
                payload,
                None,
                next_offset.is_some(),
                Value::Object(dropped),
            ))
        })
        .await
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::read_only()
    }
}

/// Typed edge list around one node.
pub struct NeighborsTool {
    root: PathBuf,
}

impl NeighborsTool {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl Tool for NeighborsTool {
    fn name(&self) -> &str {
        "neighbors"
    }

    fn description(&self) -> &str {
        "Edges touching a graph node, with an explicit edge_type and direction per row and a \
         stub (path, layer, tokens) for each neighbor. Filter by `kind` (depends_on, contains, \
         duplicate_of, similar_to, all) and `direction` (in, out, both). Read-only."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Graph node id (module path, e.g. \"crate::tools::graph\")."
                },
                "kind": {
                    "type": "string",
                    "enum": ["depends_on", "contains", "duplicate_of", "similar_to", "all"],
                    "description": "Edge type filter. Default \"all\".",
                    "default": "all"
                },
                "direction": {
                    "type": "string",
                    "enum": ["in", "out", "both"],
                    "description": "Edge direction relative to `id`. Default \"both\".",
                    "default": "both"
                }
            },
            "required": ["id"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let id = required_id(&args)?;
        let kind_raw = args
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("all")
            .to_string();
        if kind_raw != "all" && parse_edge_kind(&kind_raw).is_none() {
            bail!(
                "unknown kind '{kind_raw}' (expected depends_on|contains|duplicate_of|similar_to|all)"
            );
        }
        let kind = parse_edge_kind(&kind_raw);
        let direction = match args.get("direction").and_then(|v| v.as_str()) {
            Some(raw) => Direction::parse(raw)
                .ok_or_else(|| anyhow!("unknown direction '{raw}' (expected in|out|both)"))?,
            None => Direction::Both,
        };
        let root = self.root.clone();
        run_blocking(move || {
            let index = shared_graph_index(&root)?;
            require_node(&index, &id)?;
            let rows: Vec<Value> = index
                .directed_neighbors(&id, kind.as_ref(), direction)
                .iter()
                .map(|(other, edge_type, row_direction)| {
                    let node = index.node(other);
                    json!({
                        "id": other,
                        "path": node.and_then(|n| n.path.clone()),
                        "layer": node.map(|n| layer_name(n.layer)),
                        "tokens": node.map(|n| n.tokens).unwrap_or(0),
                        "edge_type": edge_type_name(edge_type),
                        "direction": row_direction.as_str(),
                    })
                })
                .collect();
            let mut payload = Map::new();
            payload.insert("id".into(), json!(id));
            payload.insert("kind".into(), json!(kind_raw));
            payload.insert("direction".into(), json!(direction.as_str()));
            payload.insert("count".into(), json!(rows.len()));
            payload.insert("rows".into(), Value::Array(rows));
            Ok(respond(&index, &root, payload, None, false, json!({})))
        })
        .await
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::read_only()
    }
}

/// Tests covering one node: Contains-linked test files, inline test blocks,
/// and the tests of its direct dependents ("also run").
pub struct TestMapTool {
    root: PathBuf,
}

impl TestMapTool {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl Tool for TestMapTool {
    fn name(&self) -> &str {
        "test_map"
    }

    fn description(&self) -> &str {
        "Tests covering a graph node: (a) Contains-linked test files (source: contains_edge), \
         (b) the mirrored tests/unit/<module>/ tree found by path rule (source: \
         mirror_path_rule), (c) the node's own inline test blocks (ranges, lines, tokens), and \
         (d) tests of its direct dependents as \"also_run\". Every entry carries path and \
         tokens so cargo_test invocations can be budgeted. Read-only."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Graph node id (module path, e.g. \"crate::tools::graph\")."
                }
            },
            "required": ["id"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let id = required_id(&args)?;
        let root = self.root.clone();
        run_blocking(move || {
            let index = shared_graph_index(&root)?;
            let node = require_node(&index, &id)?;
            // Symbol nodes answer through their parent file: the parent's
            // Contains/mirror tests, plus name-matched test fns inside them.
            let (query_id, query_node) = match &node.parent_id {
                Some(parent) => (parent.clone(), require_node(&index, parent)?.clone()),
                None => (id.clone(), node.clone()),
            };
            let inline = json!({
                "inline_test_ranges": query_node.inline_test_ranges,
                "inline_test_lines": query_node.inline_test_lines,
                "inline_test_tokens": query_node.inline_test_tokens,
            });
            let test_row = |test_id: &str, source: &str| {
                let test_node = index.node(test_id);
                json!({
                    "id": test_id,
                    "path": test_node.and_then(|n| n.path.clone()),
                    "tokens": test_node.map(|n| n.tokens).unwrap_or(0),
                    "lines": test_node.map(|n| n.lines).unwrap_or(0),
                    "source": source,
                })
            };
            let contains_tests: Vec<String> = index.tests_for(&query_id);
            let tests: Vec<Value> = contains_tests
                .iter()
                .map(|test_id| test_row(test_id, "contains_edge"))
                .collect();
            // Mirror-tree fallback (Contains edges miss the mirrored
            // tests/unit/<module>/ tree — the repo's #[path] test convention).
            // Labeled honestly: these come from a path rule, not graph edges.
            let (mirror_rule, mirror_tests) =
                mirror_tests_for(&index, &query_node, &contains_tests);
            // Depth-1 dependents' tests: the suites that break when `id`
            // breaks its contract, named with the dependent they hang off.
            let mut also_run: Vec<Value> = Vec::new();
            for dependent in index.dependents(&query_id) {
                for test_id in index.tests_for(dependent) {
                    let mut row = test_row(&test_id, "contains_edge");
                    row["via"] = json!(dependent);
                    also_run.push(row);
                }
            }
            let mut payload = Map::new();
            payload.insert("id".into(), json!(id));
            if let Some(parent) = &node.parent_id {
                payload.insert(
                    "symbol".into(),
                    json!({
                        "parent": parent,
                        "kind": node.symbol_kind,
                        "line_range": node.line_range,
                    }),
                );
                let linked_paths: Vec<String> = tests
                    .iter()
                    .chain(mirror_tests.iter())
                    .filter_map(|row| row["path"].as_str().map(String::from))
                    .collect();
                payload.insert(
                    "symbol_tests".into(),
                    Value::Array(symbol_tests_for(&root, node, &linked_paths)),
                );
            }
            payload.insert("tests".into(), Value::Array(tests));
            payload.insert("mirror_tests".into(), Value::Array(mirror_tests));
            if let Some(rule) = mirror_rule {
                payload.insert("mirror_rule".into(), json!(rule));
            }
            payload.insert("inline".into(), inline);
            payload.insert("also_run".into(), Value::Array(also_run));
            Ok(respond(&index, &root, payload, None, false, json!({})))
        })
        .await
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::read_only()
    }
}

/// Test fns inside the linked test files whose name mentions the symbol —
/// `test_<name>`, `<name>_test`, or any test fn named after it. Scans only
/// the test files already linked to the parent (bounded, honest about the
/// name-match heuristic via `source: "name_match"`).
fn symbol_tests_for(root: &Path, symbol: &Node, linked_paths: &[String]) -> Vec<Value> {
    let name = symbol.id.rsplit("::").next().unwrap_or(&symbol.id);
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for rel in linked_paths {
        if !seen.insert(rel.clone()) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        for (index, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            let Some(rest) = trimmed
                .strip_prefix("fn ")
                .or_else(|| trimmed.strip_prefix("pub fn "))
                .or_else(|| trimmed.strip_prefix("async fn "))
                .or_else(|| trimmed.strip_prefix("pub async fn "))
            else {
                continue;
            };
            let ident: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if ident.contains(name) {
                out.push(json!({
                    "fn": ident,
                    "file": rel,
                    "line": index + 1,
                    "source": "name_match",
                }));
            }
        }
    }
    out
}

/// Mirror-tree test probe: the repo's `#[path]` test convention maps/// `src/a/b.rs` (or `src/a/b/mod.rs`) to `tests/unit/a/b/*` with node ids
/// prefixed `test::unit::a::b::` (verified against the real graph:
/// `tests/unit/tools/graph/graph_test.rs` → `test::unit::tools::graph::graph_test`).
/// Returns the cited rule plus matching Test-layer rows, excluding ids the
/// Contains edges already cover.
fn mirror_tests_for(
    index: &GraphIndex,
    node: &Node,
    contains_tests: &[String],
) -> (Option<String>, Vec<Value>) {
    let module = node
        .path
        .as_deref()
        .and_then(|path| path.strip_prefix("src/"))
        .and_then(|path| path.strip_suffix(".rs"));
    let Some(module) = module.map(|m| m.strip_suffix("/mod").unwrap_or(m)) else {
        return (None, Vec::new());
    };
    let dir = format!("tests/unit/{module}/");
    let id_prefix = format!("test::unit::{}::", module.replace('/', "::"));
    let covered: HashSet<&str> = contains_tests.iter().map(String::as_str).collect();
    let mut rows: Vec<Value> = index
        .graph
        .nodes
        .iter()
        .filter(|candidate| candidate.layer == NodeLayer::Test)
        .filter(|candidate| !covered.contains(candidate.id.as_str()))
        .filter(|candidate| {
            candidate
                .path
                .as_deref()
                .is_some_and(|path| path.starts_with(&dir))
                || candidate.id.starts_with(&id_prefix)
        })
        .map(|candidate| {
            json!({
                "id": candidate.id,
                "path": candidate.path,
                "tokens": candidate.tokens,
                "lines": candidate.lines,
                "source": "mirror_path_rule",
            })
        })
        .collect();
    rows.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    let rule = format!(
        "src/{module}.rs → {dir}<name>_test.rs (node id prefix {id_prefix}); path-rule fallback, not graph edges"
    );
    (Some(rule), rows)
}

// ---------------------------------------------------------------------------
// cycles / dups — graph hygiene queries
// ---------------------------------------------------------------------------

const DEFAULT_CYCLES_K: usize = 20;
const DEFAULT_DUPS_K: usize = 50;

/// Dependency cycles over DependsOn edges.
pub struct CyclesTool {
    root: PathBuf,
}

impl CyclesTool {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl Tool for CyclesTool {
    fn name(&self) -> &str {
        "cycles"
    }

    fn description(&self) -> &str {
        "Dependency cycles in the evolve graph: closed DependsOn loops (own Tarjan SCC — the \
         ontology validator deliberately excludes DependsOn, so these are not reported \
         elsewhere). Each row is a node-id path that closes on itself, with per-node tokens so \
         you can budget breaking the cycle at its cheapest edge. Read-only."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "k": {
                    "type": "integer",
                    "description": "Maximum cycles. Default 20.",
                    "default": 20,
                    "minimum": 1
                }
            },
            "required": [],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let k = args
            .get("k")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_CYCLES_K as u64) as usize;
        let root = self.root.clone();
        run_blocking(move || {
            let index = shared_graph_index(&root)?;
            let cycles = index.dependency_cycles(k);
            let rows: Vec<Value> = cycles
                .iter()
                .map(|path| {
                    let nodes: Vec<Value> = path
                        .iter()
                        .map(|id| {
                            json!({
                                "id": id,
                                "tokens": index.node(id).map(|n| n.tokens).unwrap_or(0),
                            })
                        })
                        .collect();
                    // Sum unique nodes only — the path's last element repeats
                    // its first.
                    let unique: HashSet<&String> = path.iter().collect();
                    let total_tokens: usize = unique
                        .iter()
                        .map(|id| index.node(id).map(|n| n.tokens).unwrap_or(0))
                        .sum();
                    json!({
                        "path": path,
                        "length": path.len().saturating_sub(1),
                        "total_tokens": total_tokens,
                        "nodes": nodes,
                    })
                })
                .collect();
            let mut payload = Map::new();
            payload.insert("count".into(), json!(rows.len()));
            payload.insert("k".into(), json!(k));
            payload.insert("rows".into(), Value::Array(rows));
            Ok(respond(&index, &root, payload, None, false, json!({})))
        })
        .await
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::read_only()
    }
}

/// Duplicate/similar code pairs with optional drift ranking.
pub struct DupsTool {
    root: PathBuf,
}

impl DupsTool {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl Tool for DupsTool {
    fn name(&self) -> &str {
        "dups"
    }

    fn description(&self) -> &str {
        "Duplicate code pairs from the evolve graph (DuplicateOf and SimilarTo edges) with \
         per-side tokens and the token drift between copies. With `drift: true`, pairs are \
         ranked by |tokens_a - tokens_b| descending so drifting clones surface first. \
         Read-only."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "drift": {
                    "type": "boolean",
                    "description": "Rank by token drift |tokens_a - tokens_b| descending instead of lexicographic. Default false.",
                    "default": false
                },
                "k": {
                    "type": "integer",
                    "description": "Maximum pairs. Default 50.",
                    "default": 50,
                    "minimum": 1
                }
            },
            "required": [],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let drift = args.get("drift").and_then(|v| v.as_bool()).unwrap_or(false);
        let k = args
            .get("k")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_DUPS_K as u64) as usize;
        let root = self.root.clone();
        run_blocking(move || {
            let index = shared_graph_index(&root)?;
            let tokens_of = |id: &str| index.node(id).map(|n| n.tokens).unwrap_or(0);
            let mut pairs: Vec<Value> = index
                .graph
                .edges
                .iter()
                .filter(|edge| {
                    matches!(edge.edge_type, EdgeType::DuplicateOf | EdgeType::SimilarTo)
                })
                .map(|edge| {
                    let from_tokens = tokens_of(&edge.from);
                    let to_tokens = tokens_of(&edge.to);
                    json!({
                        "from": edge.from,
                        "to": edge.to,
                        "edge_type": edge_type_name(&edge.edge_type),
                        "from_tokens": from_tokens,
                        "to_tokens": to_tokens,
                        "drift": from_tokens.abs_diff(to_tokens),
                    })
                })
                .collect();
            if drift {
                pairs.sort_by(|left, right| {
                    right["drift"]
                        .as_u64()
                        .cmp(&left["drift"].as_u64())
                        .then_with(|| left["from"].as_str().cmp(&right["from"].as_str()))
                });
            } else {
                pairs.sort_by(|left, right| {
                    left["edge_type"]
                        .as_str()
                        .cmp(&right["edge_type"].as_str())
                        .then_with(|| left["from"].as_str().cmp(&right["from"].as_str()))
                        .then_with(|| left["to"].as_str().cmp(&right["to"].as_str()))
                });
            }
            let total = pairs.len();
            pairs.truncate(k);
            let dropped_count = total.saturating_sub(pairs.len());
            let mut dropped = Map::new();
            if dropped_count > 0 {
                dropped.insert("dups".into(), json!(dropped_count));
            }
            let mut payload = Map::new();
            payload.insert("drift".into(), json!(drift));
            payload.insert("count".into(), json!(pairs.len()));
            payload.insert("total_pairs".into(), json!(total));
            payload.insert("rows".into(), Value::Array(pairs));
            Ok(respond(
                &index,
                &root,
                payload,
                None,
                dropped_count > 0,
                Value::Object(dropped),
            ))
        })
        .await
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::read_only()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tools/graph/graph_test.rs"]
mod graph_test;
