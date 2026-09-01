//! Code map tools for context-aware action planning.
//!
//! Provides three agent tools:
//! - `CodeMapTool` — generates the code map for a focused area, backed by the
//!   cached evolve graph (measured per-node tokens, real `DependsOn` edges)
//!   with a live measured fallback for files the graph misses or predates
//! - `ContextBudgetTool` — reports current context token budget usage
//! - `ContextActionTool` — estimates token/time cost of an action before executing

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::Tool;

// ---------------------------------------------------------------------------
// Global shared context budget tracker
// ---------------------------------------------------------------------------
//
// ContextMap and AgentMemory both track token usage independently. To avoid
// duplicate accounting and give tools like `context_budget` a single source of
// truth, the two subsystems publish their running totals into the atomic
// counters below. `read_budget` returns the combined view.

static MEMORY_USED_TOKENS: AtomicUsize = AtomicUsize::new(0);
static CONTEXT_MAP_USED_TOKENS: AtomicUsize = AtomicUsize::new(0);
static TOTAL_BUDGET: AtomicUsize = AtomicUsize::new(1_000_000); // 1M default
static FILES_IN_CONTEXT: AtomicUsize = AtomicUsize::new(0);

/// Publish the current conversation-memory token total.
pub fn update_memory_tokens(tokens: usize) {
    MEMORY_USED_TOKENS.store(tokens, Ordering::Relaxed);
}

/// Publish the current context-map (loaded files) token total.
pub fn update_context_map_tokens(tokens: usize) {
    CONTEXT_MAP_USED_TOKENS.store(tokens, Ordering::Relaxed);
}

/// Set the overall context token budget.
pub fn update_total_budget(budget: usize) {
    TOTAL_BUDGET.store(budget, Ordering::Relaxed);
}

/// Set the number of files currently loaded in context.
pub fn update_files_in_context(files: usize) {
    FILES_IN_CONTEXT.store(files, Ordering::Relaxed);
}

/// Update all budget counters at once (convenience for tests and diagnostics).
pub fn update_budget(used: usize, total: usize, files: usize) {
    CONTEXT_MAP_USED_TOKENS.store(used, Ordering::Relaxed);
    MEMORY_USED_TOKENS.store(0, Ordering::Relaxed);
    TOTAL_BUDGET.store(total, Ordering::Relaxed);
    FILES_IN_CONTEXT.store(files, Ordering::Relaxed);
}

/// Read the current combined budget snapshot.
fn read_budget() -> (usize, usize, usize) {
    (
        MEMORY_USED_TOKENS.load(Ordering::Relaxed)
            + CONTEXT_MAP_USED_TOKENS.load(Ordering::Relaxed),
        TOTAL_BUDGET.load(Ordering::Relaxed),
        FILES_IN_CONTEXT.load(Ordering::Relaxed),
    )
}

// ---------------------------------------------------------------------------
// Action cost model
// ---------------------------------------------------------------------------

/// Actions that can be performed on code, each with a characteristic token cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextAction {
    /// Signatures only — ~6% of file tokens
    Inspect,
    /// Full file read — 100% of file tokens
    ReadFull,
    /// Skeleton view — ~10% of file tokens
    ReadSkeleton,
    /// Edit cycle (read + edit + verify) — 150%
    Alter,
    /// Scaffold + implement + test — 200%
    BuildNew,
    /// cargo check output — ~30%
    Verify,
    /// cargo test output — ~40%
    Test,
    /// git diff + commit — ~20%
    Ship,
    /// git diff output — ~15%
    GitDiff,
}

impl ContextAction {
    /// Cost multiplier relative to full file token count.
    pub fn cost_multiplier(&self) -> f64 {
        match self {
            Self::Inspect => 0.06,
            Self::ReadFull => 1.0,
            Self::ReadSkeleton => 0.10,
            Self::Alter => 1.50,
            Self::BuildNew => 2.00,
            Self::Verify => 0.30,
            Self::Test => 0.40,
            Self::Ship => 0.20,
            Self::GitDiff => 0.15,
        }
    }

    /// Whether this action triggers a cargo subprocess.
    pub fn is_cargo_op(&self) -> bool {
        matches!(self, Self::Verify | Self::Test)
    }

    /// Parse from a string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "inspect" => Some(Self::Inspect),
            "read" | "read_full" | "readfull" => Some(Self::ReadFull),
            "skeleton" | "read_skeleton" | "readskeleton" => Some(Self::ReadSkeleton),
            "alter" | "edit" => Some(Self::Alter),
            "build" | "build_new" | "buildnew" => Some(Self::BuildNew),
            "verify" | "check" => Some(Self::Verify),
            "test" => Some(Self::Test),
            "ship" | "commit" => Some(Self::Ship),
            "git_diff" | "gitdiff" | "diff" | "git" => Some(Self::GitDiff),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Fusion levels
// ---------------------------------------------------------------------------

/// How many components are involved in the operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FusionLevel {
    /// Single file operation
    Binary,
    /// File + its direct dependencies
    Trinary,
    /// File + deps + dependents (diamond pattern)
    Quaternary,
}

impl FusionLevel {
    /// Multiplier applied on top of the action cost.
    pub fn multiplier(&self) -> f64 {
        match self {
            Self::Binary => 1.0,
            Self::Trinary => 2.5,
            Self::Quaternary => 4.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Cost estimation
// ---------------------------------------------------------------------------

/// Default tokens-per-second processing rate for time estimates.
const DEFAULT_TOKENS_PER_SECOND: f64 = 3000.0;
/// I/O overhead per file in milliseconds.
const IO_OVERHEAD_MS: u64 = 500;
/// Cargo subprocess overhead in milliseconds.
const CARGO_OVERHEAD_MS: u64 = 2000;

/// Estimated cost of performing an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionCostEstimate {
    pub estimated_tokens: usize,
    pub estimated_time_ms: u64,
    pub fits_in_budget: bool,
    pub recommended_depth: u8,
}

/// Stated fallback when a file cannot be read (nonexistent / unreadable) —
/// an explicit constant, not a byte-fraction estimate.
const UNREADABLE_FILE_FALLBACK_TOKENS: usize = 500;

/// Measured token cost of a file's content (AGENTS.md rule 4: token
/// accounting goes through `estimate_content_tokens`, never byte-fraction
/// heuristics). Falls back to [`UNREADABLE_FILE_FALLBACK_TOKENS`] when the
/// file cannot be read.
fn measure_file_tokens(path: &Path) -> usize {
    match std::fs::read_to_string(path) {
        Ok(content) => crate::token_count::estimate_content_tokens(&content),
        Err(_) => UNREADABLE_FILE_FALLBACK_TOKENS,
    }
}

/// Token base for a `context_action` estimate: the evolve graph's measured
/// node tokens when the cached graph covers the file and is not older than
/// it, else a live measured read. Returns (tokens, source) so the response
/// can say which path served the number (AGENTS.md rule 3).
fn file_token_base(root: &Path, target: &Path) -> (usize, &'static str) {
    if let Ok(index) = crate::evolve::graph_cache::shared_graph_index(root) {
        let graph_mtime = std::fs::metadata(crate::evolve::graph_cache::graph_path(root))
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let fresh = target
            .metadata()
            .and_then(|m| m.modified())
            .map(|mtime| mtime <= graph_mtime)
            .unwrap_or(false);
        if fresh {
            if let Ok(rel) = target.strip_prefix(root) {
                let rel = rel.to_string_lossy();
                if let Some(node) = index
                    .graph
                    .nodes
                    .iter()
                    .find(|node| node.path.as_deref() == Some(rel.as_ref()))
                {
                    return (node.tokens, "graph");
                }
            }
        }
    }
    (measure_file_tokens(target), "live")
}

/// Estimate the cost of an action on a target path.
pub fn estimate_action_cost(
    action: ContextAction,
    target: &Path,
    fusion: FusionLevel,
) -> ActionCostEstimate {
    estimate_action_cost_with_base(action, fusion, measure_file_tokens(target))
}

/// Cost math shared by the live-measure path and the graph-backed path.
fn estimate_action_cost_with_base(
    action: ContextAction,
    fusion: FusionLevel,
    base_tokens: usize,
) -> ActionCostEstimate {
    let token_cost =
        (base_tokens as f64 * action.cost_multiplier() * fusion.multiplier()).ceil() as usize;

    let file_count = match fusion {
        FusionLevel::Binary => 1u64,
        FusionLevel::Trinary => 3,
        FusionLevel::Quaternary => 6,
    };

    let time_ms = {
        let processing = (token_cost as f64 / DEFAULT_TOKENS_PER_SECOND * 1000.0).ceil() as u64;
        let io = file_count * IO_OVERHEAD_MS;
        let cargo = if action.is_cargo_op() {
            CARGO_OVERHEAD_MS
        } else {
            0
        };
        processing + io + cargo
    };

    let (used, total, _) = read_budget();
    let remaining = total.saturating_sub(used);

    // Recommend depth based on how much budget is available
    let recommended_depth = if token_cost * 3 < remaining {
        3
    } else if token_cost * 2 < remaining {
        2
    } else {
        1
    };

    ActionCostEstimate {
        estimated_tokens: token_cost,
        estimated_time_ms: time_ms,
        fits_in_budget: token_cost <= remaining,
        recommended_depth,
    }
}

// ---------------------------------------------------------------------------
// Code graph node (lightweight)
// ---------------------------------------------------------------------------

fn is_false(value: &bool) -> bool {
    !*value
}

/// A node in the code map graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeMapNode {
    pub path: String,
    pub kind: String, // "module", "file", "function", "struct", "enum", "trait"
    pub token_estimate: usize,
    pub children: Vec<String>,
    pub dependencies: Vec<String>,
    /// True when this file was served from a LIVE measured read (tokens and
    /// deps from current disk content) because the evolve graph was missing
    /// it or predates it — a stale graph number is never served as fresh
    /// (AGENTS.md rule 3). Absent (false) when the graph served the node.
    #[serde(default, skip_serializing_if = "is_false")]
    pub live: bool,
}

/// The evolve-graph overlay for the map walk: measured per-node tokens and
/// real `DependsOn` edges, looked up by repo-relative path. Built once per
/// `build_code_map` call; `None` when no graph exists (every file then
/// takes the live path).
struct GraphOverlay<'g> {
    index: &'g crate::evolve::GraphIndex,
    graph_mtime: std::time::SystemTime,
    by_path: HashMap<&'g str, &'g crate::evolve::Node>,
}

impl<'g> GraphOverlay<'g> {
    fn load(index: &'g crate::evolve::GraphIndex, root: &Path) -> Self {
        let graph_mtime = std::fs::metadata(crate::evolve::graph_cache::graph_path(root))
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let by_path = index
            .graph
            .nodes
            .iter()
            .filter_map(|node| node.path.as_deref().map(|path| (path, node)))
            .collect();
        Self {
            index,
            graph_mtime,
            by_path,
        }
    }

    /// Token cost + dependencies for one file: the graph's measured values
    /// when the graph covers the file and is not older than it; otherwise a
    /// live measured read whose held content also feeds the dependency scan
    /// (the fallback is per-file — no full re-walk, no second cache).
    fn tokens_and_deps(&self, path: &Path, rel: &str) -> (usize, Vec<String>, bool) {
        let fresh = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .map(|mtime| mtime <= self.graph_mtime)
            .unwrap_or(false);
        if fresh {
            if let Some(node) = self.by_path.get(rel) {
                let deps = self
                    .index
                    .dependencies(&node.id)
                    .iter()
                    .map(|target| target.strip_prefix("crate::").unwrap_or(target).to_string())
                    .collect();
                return (node.tokens, deps, false);
            }
        }
        let content = std::fs::read_to_string(path).ok();
        let tokens = content
            .as_deref()
            .map(crate::token_count::estimate_content_tokens)
            .unwrap_or(UNREADABLE_FILE_FALLBACK_TOKENS);
        let deps = content.as_deref().map(extract_use_deps).unwrap_or_default();
        (tokens, deps, true)
    }
}

/// Build a lightweight code map for a directory, optionally focused on a module path.
///
/// Directory structure comes from a listing-only walk (no file reads); token
/// costs and dependency edges come from the cached evolve graph where it is
/// fresh, with per-file live fallback otherwise.
fn build_code_map(root: &Path, focus: Option<&str>, depth: u8) -> HashMap<String, CodeMapNode> {
    let mut nodes: HashMap<String, CodeMapNode> = HashMap::new();

    let scan_dir = if let Some(focus_path) = focus {
        // Resolve focus as a subpath under root/src
        let candidate = root.join("src").join(focus_path.replace("::", "/"));
        if candidate.is_dir() {
            candidate
        } else {
            // Try as a file
            let rs_file = candidate.with_extension("rs");
            if rs_file.is_file() {
                // Single file focus — add it and return
                let index = crate::evolve::graph_cache::shared_graph_index(root).ok();
                let overlay = index.as_ref().map(|index| GraphOverlay::load(index, root));
                let rel = rs_file
                    .strip_prefix(root)
                    .unwrap_or(&rs_file)
                    .to_string_lossy()
                    .to_string();
                let (tokens, dependencies, live) = match &overlay {
                    Some(overlay) => overlay.tokens_and_deps(&rs_file, &rel),
                    None => {
                        let content = std::fs::read_to_string(&rs_file).ok();
                        (
                            content
                                .as_deref()
                                .map(crate::token_count::estimate_content_tokens)
                                .unwrap_or(UNREADABLE_FILE_FALLBACK_TOKENS),
                            content.as_deref().map(extract_use_deps).unwrap_or_default(),
                            true,
                        )
                    }
                };
                nodes.insert(
                    rel.clone(),
                    CodeMapNode {
                        path: rel,
                        kind: "file".to_string(),
                        token_estimate: tokens,
                        children: Vec::new(),
                        dependencies,
                        live,
                    },
                );
                return nodes;
            }
            root.join("src")
        }
    } else {
        root.join("src")
    };

    if !scan_dir.exists() {
        return nodes;
    }

    let index = crate::evolve::graph_cache::shared_graph_index(root).ok();
    let overlay = index.as_ref().map(|index| GraphOverlay::load(index, root));
    let overlay = overlay.as_ref();
    collect_rs_files(&scan_dir, root, depth, 0, overlay, &mut nodes);
    nodes
}

/// Recursively collect .rs files up to a given depth (directory listing
/// only — file contents are read solely by the per-file live fallback).
fn collect_rs_files(
    dir: &Path,
    root: &Path,
    max_depth: u8,
    current_depth: u8,
    overlay: Option<&GraphOverlay>,
    nodes: &mut HashMap<String, CodeMapNode>,
) {
    if current_depth > max_depth {
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            nodes.insert(
                rel.clone(),
                CodeMapNode {
                    path: rel.clone(),
                    kind: "module".to_string(),
                    token_estimate: 0,
                    children: Vec::new(),
                    dependencies: Vec::new(),
                    live: false,
                },
            );
            collect_rs_files(&path, root, max_depth, current_depth + 1, overlay, nodes);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            let (tokens, dependencies, live) = match overlay {
                Some(overlay) => overlay.tokens_and_deps(&path, &rel),
                None => {
                    let content = std::fs::read_to_string(&path).ok();
                    (
                        content
                            .as_deref()
                            .map(crate::token_count::estimate_content_tokens)
                            .unwrap_or(UNREADABLE_FILE_FALLBACK_TOKENS),
                        content.as_deref().map(extract_use_deps).unwrap_or_default(),
                        true,
                    )
                }
            };

            // Find parent module node and add this as a child
            if let Some(parent_rel) = path
                .parent()
                .and_then(|p| p.strip_prefix(root).ok())
                .map(|p| p.to_string_lossy().to_string())
            {
                if let Some(parent_node) = nodes.get_mut(&parent_rel) {
                    parent_node.children.push(rel.clone());
                    parent_node.token_estimate += tokens;
                }
            }

            nodes.insert(
                rel.clone(),
                CodeMapNode {
                    path: rel,
                    kind: "file".to_string(),
                    token_estimate: tokens,
                    children: Vec::new(),
                    dependencies,
                    live,
                },
            );
        }
    }
}

/// Extract `use crate::...` dependency paths from Rust source (quick scan,
/// no full parse). Only used for files served by the live fallback — files
/// the graph covers get real `DependsOn` edges instead.
fn extract_use_deps(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("use crate::") {
                let end = rest.find([';', '{', ' ']).unwrap_or(rest.len());
                Some(rest[..end].to_string())
            } else {
                None
            }
        })
        .collect()
}

// ===========================================================================
// Tool: CodeMapTool
// ===========================================================================

/// Generates the code graph on-demand for a focused area.
#[derive(Default)]
pub struct CodeMapTool;

#[async_trait]
impl Tool for CodeMapTool {
    fn name(&self) -> &str {
        "code_map"
    }

    fn description(&self) -> &str {
        "Generate a code graph for the project or a focused module. Returns graph nodes with \
         token cost estimates and dependency edges. Use `focus` to narrow to a module path \
         (e.g. \"tools::codemap\"), `depth` (1-3) to control recursion, and `format` \
         (json/summary) to choose output shape."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "focus": {
                    "type": "string",
                    "description": "Module path to focus on (e.g. \"tools::codemap\"). Omit for full project."
                },
                "depth": {
                    "type": "integer",
                    "description": "Recursion depth 1-3. Default 2.",
                    "minimum": 1,
                    "maximum": 3,
                    "default": 2
                },
                "format": {
                    "type": "string",
                    "enum": ["json", "summary"],
                    "description": "Output format. Default \"summary\".",
                    "default": "summary"
                }
            },
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let focus = args.get("focus").and_then(|v| v.as_str()).map(String::from);
        let depth = args
            .get("depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(2)
            .min(3) as u8;
        let format = args
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("summary")
            .to_string();

        // The first shared_graph_index call may parse the graph YAML — keep
        // it off the async hot path (subsequent calls are stat-only).
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let focus_for_closure = focus.clone();
        let nodes = tokio::task::spawn_blocking(move || {
            build_code_map(&root, focus_for_closure.as_deref(), depth)
        })
        .await
        .map_err(|e| anyhow::anyhow!("code_map task failed: {e}"))?;

        let total_tokens: usize = nodes
            .values()
            .filter(|n| n.kind == "file")
            .map(|n| n.token_estimate)
            .sum();

        if format == "json" {
            Ok(json!({
                "nodes": nodes,
                "total_token_estimate": total_tokens,
                "node_count": nodes.len(),
                "focus": focus,
                "depth": depth,
            }))
        } else {
            // Summary format: compact text
            let mut lines = Vec::new();
            lines.push(format!(
                "Code Map: {} nodes, ~{} tokens",
                nodes.len(),
                total_tokens
            ));
            if let Some(f) = focus {
                lines.push(format!("Focus: {}", f));
            }
            lines.push(format!("Depth: {}", depth));
            lines.push(String::new());

            let mut sorted_keys: Vec<_> = nodes.keys().collect();
            sorted_keys.sort();

            for key in sorted_keys {
                let node = &nodes[key];
                let prefix = if node.kind == "module" {
                    "dir "
                } else {
                    "    "
                };
                let deps = if node.dependencies.is_empty() {
                    String::new()
                } else {
                    format!(" -> [{}]", node.dependencies.join(", "))
                };
                // Honesty marker: this node's numbers came from a live read
                // because the graph was missing the file or predates it.
                let live_marker = if node.live { " [live]" } else { "" };
                lines.push(format!(
                    "{}{} (~{} tok){}{}",
                    prefix, node.path, node.token_estimate, deps, live_marker
                ));
            }

            Ok(json!({ "summary": lines.join("\n") }))
        }
    }
}

// ===========================================================================
// Tool: ContextBudgetTool
// ===========================================================================

/// Reports the current context token budget status.
#[derive(Default)]
pub struct ContextBudgetTool;

#[async_trait]
impl Tool for ContextBudgetTool {
    fn name(&self) -> &str {
        "context_budget"
    }

    fn description(&self) -> &str {
        "Show current context token budget: how many tokens are used, total budget, \
         files loaded, and how many actions can still fit."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    async fn execute(&self, _args: Value) -> Result<Value> {
        let (used, total, files) = read_budget();
        let remaining = total.saturating_sub(used);

        // Estimate how many "typical" actions fit: assume a medium file (~2000 tokens)
        // with an Alter action (1.5x) at Binary fusion (1.0x) = 3000 tokens
        let typical_action_cost = 3000usize;
        let actions_available = remaining.checked_div(typical_action_cost).unwrap_or(0);

        Ok(json!({
            "used_tokens": used,
            "total_budget": total,
            "remaining_tokens": remaining,
            "files_in_context": files,
            "actions_available": actions_available,
            "utilization_pct": if total > 0 {
                (used as f64 / total as f64 * 100.0).round() as u64
            } else {
                0
            },
        }))
    }
}

// ===========================================================================
// Tool: ContextActionTool
// ===========================================================================

/// Estimates the cost of an action before executing it.
#[derive(Default)]
pub struct ContextActionTool;

#[async_trait]
impl Tool for ContextActionTool {
    fn name(&self) -> &str {
        "context_action"
    }

    fn description(&self) -> &str {
        "Estimate the token and time cost of a code action before executing it. \
         Supports actions: inspect, read, skeleton, alter, build, verify, test, ship, git. \
         Returns estimated tokens, time, whether it fits in budget, and recommended depth."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Action to estimate: inspect, read, skeleton, alter, build, verify, test, ship, git",
                    "enum": ["inspect", "read", "skeleton", "alter", "build", "verify", "test", "ship", "git"]
                },
                "target": {
                    "type": "string",
                    "description": "File path or module path to act on (e.g. \"src/tools/codemap.rs\" or \"tools::codemap\")"
                },
                "fusion": {
                    "type": "string",
                    "enum": ["binary", "trinary", "quaternary"],
                    "description": "Fusion level: binary (single file), trinary (file + deps), quaternary (file + deps + dependents). Default binary.",
                    "default": "binary"
                }
            },
            "required": ["action", "target"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let action_str = args
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing required field: action"))?;

        let target_str = args
            .get("target")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing required field: target"))?;

        let fusion_str = args
            .get("fusion")
            .and_then(|v| v.as_str())
            .unwrap_or("binary");

        let action = ContextAction::from_str_loose(action_str)
            .ok_or_else(|| anyhow::anyhow!("unknown action: {}", action_str))?;

        let fusion = match fusion_str {
            "trinary" => FusionLevel::Trinary,
            "quaternary" => FusionLevel::Quaternary,
            _ => FusionLevel::Binary,
        };

        // Resolve target path: if it looks like a module path, convert to file path
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let target_path = if target_str.contains("::") {
            let file_rel = target_str.replace("::", "/");
            let candidate = root.join("src").join(&file_rel).with_extension("rs");
            if candidate.is_file() {
                candidate
            } else {
                // Try as directory mod.rs
                let mod_candidate = root.join("src").join(&file_rel).join("mod.rs");
                if mod_candidate.is_file() {
                    mod_candidate
                } else {
                    PathBuf::from(target_str)
                }
            }
        } else {
            let p = PathBuf::from(target_str);
            if p.is_absolute() {
                p
            } else {
                root.join(target_str)
            }
        };

        let (base_tokens, token_source) = file_token_base(&root, &target_path);
        let estimate = estimate_action_cost_with_base(action, fusion, base_tokens);

        Ok(json!({
            "action": action_str,
            "target": target_str,
            "fusion": fusion_str,
            "estimated_tokens": estimate.estimated_tokens,
            "estimated_time_ms": estimate.estimated_time_ms,
            "fits_in_budget": estimate.fits_in_budget,
            "recommended_depth": estimate.recommended_depth,
            "cost_multiplier": action.cost_multiplier(),
            "fusion_multiplier": fusion.multiplier(),
            "token_source": token_source,
        }))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tools/codemap/codemap_test.rs"]
mod tests;
