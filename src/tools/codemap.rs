//! Code map tools for context-aware action planning.
//!
//! Provides three agent tools:
//! - `CodeMapTool` — generates the code graph on-demand for a focused area
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
// Global context budget tracker (simple atomic counters)
// ---------------------------------------------------------------------------

static USED_TOKENS: AtomicUsize = AtomicUsize::new(0);
static TOTAL_BUDGET: AtomicUsize = AtomicUsize::new(1_000_000); // 1M default
static FILES_IN_CONTEXT: AtomicUsize = AtomicUsize::new(0);

/// Update the global context budget counters.
pub fn update_budget(used: usize, total: usize, files: usize) {
    USED_TOKENS.store(used, Ordering::Relaxed);
    TOTAL_BUDGET.store(total, Ordering::Relaxed);
    FILES_IN_CONTEXT.store(files, Ordering::Relaxed);
}

/// Read the current budget snapshot.
fn read_budget() -> (usize, usize, usize) {
    (
        USED_TOKENS.load(Ordering::Relaxed),
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

/// Estimate the token cost for a file, using ~4 chars per token heuristic.
fn estimate_file_tokens(path: &Path) -> usize {
    match std::fs::metadata(path) {
        Ok(meta) => (meta.len() as usize) / 4,
        Err(_) => 500, // fallback for non-existent / unreadable
    }
}

/// Estimate the cost of an action on a target path.
pub fn estimate_action_cost(
    action: ContextAction,
    target: &Path,
    fusion: FusionLevel,
) -> ActionCostEstimate {
    let base_tokens = estimate_file_tokens(target);
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

/// A node in the code map graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeMapNode {
    pub path: String,
    pub kind: String, // "module", "file", "function", "struct", "enum", "trait"
    pub token_estimate: usize,
    pub children: Vec<String>,
    pub dependencies: Vec<String>,
}

/// Build a lightweight code map for a directory, optionally focused on a module path.
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
                let tokens = estimate_file_tokens(&rs_file);
                let rel = rs_file
                    .strip_prefix(root)
                    .unwrap_or(&rs_file)
                    .to_string_lossy()
                    .to_string();
                nodes.insert(
                    rel.clone(),
                    CodeMapNode {
                        path: rel,
                        kind: "file".to_string(),
                        token_estimate: tokens,
                        children: Vec::new(),
                        dependencies: extract_use_deps(&rs_file),
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

    collect_rs_files(&scan_dir, root, depth, 0, &mut nodes);
    nodes
}

/// Recursively collect .rs files up to a given depth.
fn collect_rs_files(
    dir: &Path,
    root: &Path,
    max_depth: u8,
    current_depth: u8,
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
                },
            );
            collect_rs_files(&path, root, max_depth, current_depth + 1, nodes);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let tokens = estimate_file_tokens(&path);
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();

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
                    dependencies: extract_use_deps(&path),
                },
            );
        }
    }
}

/// Extract `use crate::...` dependency paths from a Rust file (quick scan, no full parse).
fn extract_use_deps(path: &Path) -> Vec<String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

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
        let focus = args.get("focus").and_then(|v| v.as_str());
        let depth = args
            .get("depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(2)
            .min(3) as u8;
        let format = args
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("summary");

        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let nodes = build_code_map(&root, focus, depth);

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
                lines.push(format!(
                    "{}{} (~{} tok){}",
                    prefix, node.path, node.token_estimate, deps
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

        let estimate = estimate_action_cost(action, &target_path, fusion);

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
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_cost_multipliers() {
        assert!((ContextAction::Inspect.cost_multiplier() - 0.06).abs() < f64::EPSILON);
        assert!((ContextAction::ReadFull.cost_multiplier() - 1.0).abs() < f64::EPSILON);
        assert!((ContextAction::Alter.cost_multiplier() - 1.5).abs() < f64::EPSILON);
        assert!((ContextAction::BuildNew.cost_multiplier() - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_fusion_multipliers() {
        assert!((FusionLevel::Binary.multiplier() - 1.0).abs() < f64::EPSILON);
        assert!((FusionLevel::Trinary.multiplier() - 2.5).abs() < f64::EPSILON);
        assert!((FusionLevel::Quaternary.multiplier() - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_action_from_str_loose() {
        assert_eq!(
            ContextAction::from_str_loose("inspect"),
            Some(ContextAction::Inspect)
        );
        assert_eq!(
            ContextAction::from_str_loose("READ"),
            Some(ContextAction::ReadFull)
        );
        assert_eq!(
            ContextAction::from_str_loose("edit"),
            Some(ContextAction::Alter)
        );
        assert_eq!(
            ContextAction::from_str_loose("commit"),
            Some(ContextAction::Ship)
        );
        assert_eq!(ContextAction::from_str_loose("unknown"), None);
    }

    #[test]
    fn test_is_cargo_op() {
        assert!(ContextAction::Verify.is_cargo_op());
        assert!(ContextAction::Test.is_cargo_op());
        assert!(!ContextAction::Inspect.is_cargo_op());
        assert!(!ContextAction::Alter.is_cargo_op());
    }

    #[test]
    fn test_estimate_action_cost_nonexistent() {
        let est = estimate_action_cost(
            ContextAction::ReadFull,
            Path::new("/nonexistent/file.rs"),
            FusionLevel::Binary,
        );
        // Should use fallback of 500 tokens
        assert_eq!(est.estimated_tokens, 500);
        assert!(est.fits_in_budget);
    }

    #[test]
    fn test_estimate_action_cost_fusion_scaling() {
        let binary = estimate_action_cost(
            ContextAction::ReadFull,
            Path::new("/nonexistent/file.rs"),
            FusionLevel::Binary,
        );
        let trinary = estimate_action_cost(
            ContextAction::ReadFull,
            Path::new("/nonexistent/file.rs"),
            FusionLevel::Trinary,
        );
        let quaternary = estimate_action_cost(
            ContextAction::ReadFull,
            Path::new("/nonexistent/file.rs"),
            FusionLevel::Quaternary,
        );

        assert!(trinary.estimated_tokens > binary.estimated_tokens);
        assert!(quaternary.estimated_tokens > trinary.estimated_tokens);
    }

    #[test]
    fn test_cargo_op_adds_overhead() {
        let verify = estimate_action_cost(
            ContextAction::Verify,
            Path::new("/nonexistent/file.rs"),
            FusionLevel::Binary,
        );
        let inspect = estimate_action_cost(
            ContextAction::Inspect,
            Path::new("/nonexistent/file.rs"),
            FusionLevel::Binary,
        );
        // Verify should have more time due to cargo overhead
        assert!(verify.estimated_time_ms > inspect.estimated_time_ms);
    }

    #[test]
    fn test_update_and_read_budget() {
        update_budget(50_000, 200_000, 10);
        let (used, total, files) = read_budget();
        assert_eq!(used, 50_000);
        assert_eq!(total, 200_000);
        assert_eq!(files, 10);
    }

    #[test]
    fn test_extract_use_deps_empty() {
        let deps = extract_use_deps(Path::new("/nonexistent/file.rs"));
        assert!(deps.is_empty());
    }
}
