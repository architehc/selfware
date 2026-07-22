//! Task-aware context selection.
//!
//! Different evolve tasks need different slices of the codebase. Rather than a
//! fixed Lite/Full/FullExtended mode, this picks source *intelligently* from the
//! graph based on the task kind:
//!
//! - `Extend` / `Refactor <target>` — the target's own source, its **dependents**
//!   (the blast radius you must not break) and its **dependencies** (as context).
//! - `Consolidate` — files that carry duplicate functions (from `fn_dedup`).
//! - `Cleanup` — files with reachability-dead code (from `dead_code`).
//! - `Understand <concept>` — the concept's grounded neighborhood.
//!
//! The result is a ranked list of files with the role each plays and why, so a
//! caller (the assistant, or an external agent) loads exactly what the task needs.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use anyhow::{bail, Result};
use serde::Serialize;

use super::{DeadCodeAnalyzer, EdgeType, FnDedupAnalyzer, Graph};

/// The kind of evolution work, which determines what context is relevant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    Extend,
    Refactor,
    Consolidate,
    Cleanup,
    Understand,
}

impl TaskKind {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s.trim().to_lowercase().as_str() {
            "extend" => Self::Extend,
            "refactor" => Self::Refactor,
            "consolidate" | "dedup" => Self::Consolidate,
            "cleanup" | "dead_code" | "prune" => Self::Cleanup,
            "understand" | "explain" => Self::Understand,
            other => bail!("unknown task kind: {other}"),
        })
    }
}

/// One file selected into the task context, with the role it plays.
#[derive(Debug, Clone, Serialize)]
pub struct SelectedFile {
    pub path: String,
    /// seed | dependent | dependency | finding
    pub role: String,
    pub reason: String,
}

/// The curated context for a task.
#[derive(Debug, Clone, Serialize)]
pub struct ContextSelection {
    pub task_kind: String,
    pub target: String,
    pub files: Vec<SelectedFile>,
    pub rationale: String,
}

/// Select the relevant source for `kind`/`target`, using the code graph and (for
/// consolidate/cleanup) the findings analyzers rooted at `project_root`.
pub fn select(
    kind: TaskKind,
    target: &str,
    graph: &Graph,
    project_root: &Path,
) -> Result<ContextSelection> {
    let files = match kind {
        TaskKind::Extend | TaskKind::Refactor | TaskKind::Understand => {
            neighborhood(graph, target)
        }
        TaskKind::Consolidate => consolidate_files(project_root)?,
        TaskKind::Cleanup => cleanup_files(project_root)?,
    };
    let rationale = match kind {
        TaskKind::Extend => "target source + dependents (callers to preserve) + dependency interfaces",
        TaskKind::Refactor => "target source + full dependent set (the blast radius to keep green)",
        TaskKind::Understand => "target source + its immediate graph neighborhood",
        TaskKind::Consolidate => "files sharing duplicate functions — the consolidation surface",
        TaskKind::Cleanup => "files with reachability-dead symbols — the prune surface",
    };
    Ok(ContextSelection {
        task_kind: format!("{kind:?}").to_lowercase(),
        target: target.to_string(),
        files,
        rationale: rationale.to_string(),
    })
}

/// The graph neighborhood of a target node: seed(s), inbound dependents, and
/// outbound dependencies. Node ids/paths are matched by substring.
fn neighborhood(graph: &Graph, target: &str) -> Vec<SelectedFile> {
    let t = target.to_lowercase();
    let mut seeds: HashSet<&str> = graph
        .nodes
        .iter()
        .filter(|n| {
            n.id.to_lowercase().contains(&t)
                || n.path.as_deref().is_some_and(|p| p.to_lowercase().contains(&t))
        })
        .map(|n| n.id.as_str())
        .collect();

    // Fallback: a target like `ToolRegistry` names a type/function, not a module
    // path. Seed on files that DEFINE it (a `struct/enum/trait/fn <target>`), so
    // type- and function-level targets resolve too.
    if seeds.is_empty() && !target.is_empty() {
        let needles = [
            format!("struct {target}"),
            format!("enum {target}"),
            format!("trait {target}"),
            format!("fn {target}"),
            format!("type {target}"),
        ];
        for node in &graph.nodes {
            if let Some(path) = node.path.as_deref() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if needles.iter().any(|n| content.contains(n.as_str())) {
                        seeds.insert(node.id.as_str());
                    }
                }
            }
        }
    }

    let path_of: BTreeMap<&str, &str> = graph
        .nodes
        .iter()
        .filter_map(|n| n.path.as_deref().map(|p| (n.id.as_str(), p)))
        .collect();

    let mut out: BTreeMap<String, SelectedFile> = BTreeMap::new();
    let mut add = |id: &str, role: &str, reason: String| {
        if let Some(&path) = path_of.get(id) {
            out.entry(path.to_string()).or_insert(SelectedFile {
                path: path.to_string(),
                role: role.to_string(),
                reason,
            });
        }
    };
    for &s in &seeds {
        add(s, "seed", "matches the task target".to_string());
    }
    for edge in &graph.edges {
        if !matches!(edge.edge_type, EdgeType::DependsOn) {
            continue;
        }
        if seeds.contains(edge.to.as_str()) {
            add(&edge.from, "dependent", format!("depends on {} — must stay green", edge.to));
        }
        if seeds.contains(edge.from.as_str()) {
            add(&edge.to, "dependency", format!("used by {} — needed as context", edge.from));
        }
    }
    out.into_values().collect()
}

fn consolidate_files(root: &Path) -> Result<Vec<SelectedFile>> {
    let pairs = FnDedupAnalyzer::new(root.join("src")).find()?;
    let mut by_path: BTreeMap<String, usize> = BTreeMap::new();
    for p in &pairs {
        *by_path.entry(p.first.path.clone()).or_default() += 1;
        *by_path.entry(p.second.path.clone()).or_default() += 1;
    }
    Ok(by_path
        .into_iter()
        .map(|(path, n)| SelectedFile {
            path,
            role: "finding".to_string(),
            reason: format!("{n} duplicate-function pair(s)"),
        })
        .collect())
}

fn cleanup_files(root: &Path) -> Result<Vec<SelectedFile>> {
    let dead = DeadCodeAnalyzer::new(root).find()?;
    let mut by_path: BTreeMap<String, usize> = BTreeMap::new();
    for d in dead.iter().filter(|d| !d.cfg_gated) {
        *by_path.entry(d.path.clone()).or_default() += 1;
    }
    Ok(by_path
        .into_iter()
        .map(|(path, n)| SelectedFile {
            path,
            role: "finding".to_string(),
            reason: format!("{n} dead symbol(s)"),
        })
        .collect())
}
