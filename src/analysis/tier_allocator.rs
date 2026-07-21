//! DAG-aware context tier allocator.
//!
//! Given a code dependency graph, a focus node, and a token budget (derived from
//! the model's context window), assigns each reachable file a context tier:
//!
//! - **L4 (Edit)**: The focused file — full detail with architecture notes,
//!   unsafe blocks, refactor risks. ~800 tokens.
//! - **L3 (Integrate)**: Direct imports/exports of the focus — type boundaries,
//!   trait impls, breaking-change surface. ~400 tokens.
//! - **L2 (Work)**: 2-hop neighbors — struct/enum signatures, entry points,
//!   error patterns. ~200 tokens.
//! - **L1 (Describe)**: Everything else reachable — role, public surface count,
//!   dependency list. ~50 tokens.
//!
//! Files unreachable from the focus node are excluded entirely.
//!
//! The allocator is parameterized by context window size, making it work for
//! both 9B models (32K context) and 27B+ models (128K+ context).

use std::collections::{HashMap, HashSet, VecDeque};

use super::code_graph::CodeGraph;

/// Context tier assigned to a file based on its DAG distance from the focus node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContextTier {
    /// L1: Role + public surface count + dependency list (~50 tokens)
    Describe,
    /// L2: Struct/enum signatures, entry points, error patterns (~200 tokens)
    Work,
    /// L3: Import/export boundaries, trait impls, shared types (~400 tokens)
    Integrate,
    /// L4: Full architecture detail, unsafe blocks, test coverage (~800 tokens)
    Edit,
}

impl ContextTier {
    /// Default token budget per file at this tier.
    pub fn default_tokens(&self) -> usize {
        match self {
            ContextTier::Describe => 50,
            ContextTier::Work => 200,
            ContextTier::Integrate => 400,
            ContextTier::Edit => 800,
        }
    }

    /// Returns the tier for a given hop distance from the focus node.
    pub fn from_hops(hops: usize) -> Self {
        match hops {
            0 => ContextTier::Edit,
            1 => ContextTier::Integrate,
            2 => ContextTier::Work,
            _ => ContextTier::Describe,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ContextTier::Describe => "L1-Describe",
            ContextTier::Work => "L2-Work",
            ContextTier::Integrate => "L3-Integrate",
            ContextTier::Edit => "L4-Edit",
        }
    }
}

impl std::fmt::Display for ContextTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A single file's tier assignment with its estimated token cost.
#[derive(Debug, Clone)]
pub struct TierAssignment {
    /// Node ID in the CodeGraph.
    pub node_id: String,
    /// File path (if the node has one).
    pub file_path: Option<String>,
    /// Display name of the node.
    pub name: String,
    /// Assigned context tier.
    pub tier: ContextTier,
    /// Hop distance from the focus node in the DAG.
    pub hops: usize,
    /// Estimated token cost at this tier.
    pub estimated_tokens: usize,
}

/// Result of a tier allocation: assignments + budget summary.
#[derive(Debug, Clone)]
pub struct TierAllocation {
    /// The focus node ID.
    pub focus_node: String,
    /// Per-node tier assignments, ordered by tier (L4 first, then L3, L2, L1).
    pub assignments: Vec<TierAssignment>,
    /// Total estimated tokens across all assignments.
    pub total_tokens: usize,
    /// Available token budget (context_window - system_prompt - output_reserve).
    pub budget: usize,
    /// Number of nodes excluded because they'd exceed the budget.
    pub excluded_count: usize,
    /// Number of nodes downgraded to a lower tier to fit budget.
    pub downgraded_count: usize,
}

impl TierAllocation {
    /// Get assignments at a specific tier.
    pub fn at_tier(&self, tier: ContextTier) -> Vec<&TierAssignment> {
        self.assignments.iter().filter(|a| a.tier == tier).collect()
    }

    /// Get the assignment for a specific node.
    pub fn for_node(&self, node_id: &str) -> Option<&TierAssignment> {
        self.assignments.iter().find(|a| a.node_id == node_id)
    }

    /// Budget utilization as a percentage.
    pub fn utilization_pct(&self) -> f64 {
        if self.budget == 0 {
            return 0.0;
        }
        (self.total_tokens as f64 / self.budget as f64 * 100.0).min(100.0)
    }

    /// Human-readable summary.
    pub fn summary(&self) -> String {
        let l4 = self.at_tier(ContextTier::Edit).len();
        let l3 = self.at_tier(ContextTier::Integrate).len();
        let l2 = self.at_tier(ContextTier::Work).len();
        let l1 = self.at_tier(ContextTier::Describe).len();
        format!(
            "Focus: {} | L4:{} L3:{} L2:{} L1:{} | {}/{} tokens ({:.0}%) | excluded:{} downgraded:{}",
            self.focus_node,
            l4, l3, l2, l1,
            self.total_tokens, self.budget,
            self.utilization_pct(),
            self.excluded_count,
            self.downgraded_count,
        )
    }
}

/// Configuration for the tier allocator.
#[derive(Debug, Clone)]
pub struct TierAllocatorConfig {
    /// Total context window size in tokens (e.g. 32768 for 9B, 131072 for 27B).
    pub context_window: usize,
    /// Tokens reserved for system prompt + tool definitions.
    pub system_reserve: usize,
    /// Tokens reserved for model output (response + thinking).
    pub output_reserve: usize,
    /// Per-tier token budgets. Override defaults if needed.
    pub tier_tokens: [usize; 4], // [L1, L2, L3, L4]
}

impl TierAllocatorConfig {
    /// Create config for a given context window size.
    /// Automatically scales reserves for the model size.
    pub fn for_context_window(context_window: usize) -> Self {
        // System prompt + tools typically ~2K for small models, ~4K for large
        let system_reserve = if context_window <= 32768 { 2000 } else { 4000 };
        // Reserve ~40% for output + thinking on small models, ~30% on large
        let output_pct = if context_window <= 32768 { 0.40 } else { 0.30 };
        let output_reserve = (context_window as f64 * output_pct) as usize;

        Self {
            context_window,
            system_reserve,
            output_reserve,
            tier_tokens: [
                ContextTier::Describe.default_tokens(),
                ContextTier::Work.default_tokens(),
                ContextTier::Integrate.default_tokens(),
                ContextTier::Edit.default_tokens(),
            ],
        }
    }

    /// Available tokens for context content (after reserves).
    pub fn content_budget(&self) -> usize {
        self.context_window
            .saturating_sub(self.system_reserve)
            .saturating_sub(self.output_reserve)
    }

    /// Token cost for a file at the given tier.
    pub fn tokens_for_tier(&self, tier: ContextTier) -> usize {
        match tier {
            ContextTier::Describe => self.tier_tokens[0],
            ContextTier::Work => self.tier_tokens[1],
            ContextTier::Integrate => self.tier_tokens[2],
            ContextTier::Edit => self.tier_tokens[3],
        }
    }
}

impl Default for TierAllocatorConfig {
    fn default() -> Self {
        Self::for_context_window(131072) // 128K default
    }
}

/// Compute BFS hop distances from a source node in the CodeGraph.
///
/// Traverses both outgoing (dependencies) and incoming (dependents) edges
/// to find the full connected component, since context relevance is
/// bidirectional: a file you import AND a file that imports you both matter.
fn bfs_hop_distances(graph: &CodeGraph, source_id: &str) -> HashMap<String, usize> {
    let mut distances: HashMap<String, usize> = HashMap::new();
    let mut queue: VecDeque<String> = VecDeque::new();

    distances.insert(source_id.to_string(), 0);
    queue.push_back(source_id.to_string());

    while let Some(current) = queue.pop_front() {
        let current_dist = distances[&current];

        // Follow outgoing edges (this node depends on target)
        for dep in graph.dependencies(&current) {
            if !distances.contains_key(&dep.id) {
                distances.insert(dep.id.clone(), current_dist + 1);
                queue.push_back(dep.id.clone());
            }
        }

        // Follow incoming edges (target depends on this node)
        for dep in graph.dependents(&current) {
            if !distances.contains_key(&dep.id) {
                distances.insert(dep.id.clone(), current_dist + 1);
                queue.push_back(dep.id.clone());
            }
        }
    }

    distances
}

/// Allocate context tiers for all nodes reachable from `focus_node_id` in the
/// graph, respecting the token budget.
///
/// Algorithm:
/// 1. BFS from focus node to get hop distances for all reachable nodes.
/// 2. Assign initial tiers based on hop distance (0→L4, 1→L3, 2→L2, 3+→L1).
/// 3. If total exceeds budget, progressively downgrade furthest nodes first.
/// 4. If still over budget, exclude nodes starting from the most distant.
pub fn allocate_tiers(
    graph: &CodeGraph,
    focus_node_id: &str,
    config: &TierAllocatorConfig,
) -> TierAllocation {
    let budget = config.content_budget();
    let distances = bfs_hop_distances(graph, focus_node_id);

    if distances.is_empty() {
        return TierAllocation {
            focus_node: focus_node_id.to_string(),
            assignments: Vec::new(),
            total_tokens: 0,
            budget,
            excluded_count: 0,
            downgraded_count: 0,
        };
    }

    // Step 1: Build initial assignments sorted by hop distance (closest first).
    let mut assignments: Vec<TierAssignment> = distances
        .iter()
        .filter_map(|(node_id, &hops)| {
            let node = graph.get_node_by_id(node_id)?;
            let tier = ContextTier::from_hops(hops);
            Some(TierAssignment {
                node_id: node_id.clone(),
                file_path: node.file_path.clone(),
                name: node.name.clone(),
                tier,
                hops,
                estimated_tokens: config.tokens_for_tier(tier),
            })
        })
        .collect();

    // Sort: focus node first, then by ascending hop distance.
    assignments.sort_by_key(|a| (a.hops, a.name.clone()));

    // Step 2: Calculate total. If within budget, we're done.
    let mut total: usize = assignments.iter().map(|a| a.estimated_tokens).sum();
    let mut downgraded_count = 0;
    let mut excluded_count = 0;

    // Step 3: Downgrade furthest nodes to cheaper tiers if over budget.
    if total > budget {
        // Process from most distant to closest, never downgrade the focus node.
        let mut indices_by_distance: Vec<usize> = (0..assignments.len()).collect();
        indices_by_distance.sort_by(|&a, &b| assignments[b].hops.cmp(&assignments[a].hops));

        for &idx in &indices_by_distance {
            if total <= budget {
                break;
            }
            // Never downgrade the focus node (hops == 0).
            if assignments[idx].hops == 0 {
                continue;
            }

            let current_tier = assignments[idx].tier;
            let downgraded_tier = match current_tier {
                ContextTier::Edit => Some(ContextTier::Integrate),
                ContextTier::Integrate => Some(ContextTier::Work),
                ContextTier::Work => Some(ContextTier::Describe),
                ContextTier::Describe => None, // Can't downgrade further
            };

            if let Some(new_tier) = downgraded_tier {
                let old_tokens = assignments[idx].estimated_tokens;
                let new_tokens = config.tokens_for_tier(new_tier);
                assignments[idx].tier = new_tier;
                assignments[idx].estimated_tokens = new_tokens;
                total = total.saturating_sub(old_tokens) + new_tokens;
                downgraded_count += 1;
            }
        }
    }

    // Step 4: If still over budget after downgrading, exclude most distant nodes.
    if total > budget {
        // Sort by distance descending for exclusion.
        let mut indices_by_distance: Vec<usize> = (0..assignments.len()).collect();
        indices_by_distance.sort_by(|&a, &b| assignments[b].hops.cmp(&assignments[a].hops));

        let mut to_exclude: HashSet<usize> = HashSet::new();
        for &idx in &indices_by_distance {
            if total <= budget {
                break;
            }
            if assignments[idx].hops == 0 {
                continue; // Never exclude the focus node.
            }
            total = total.saturating_sub(assignments[idx].estimated_tokens);
            to_exclude.insert(idx);
            excluded_count += 1;
        }

        // Remove excluded assignments (in reverse order to keep indices valid).
        let mut excluded_sorted: Vec<usize> = to_exclude.into_iter().collect();
        excluded_sorted.sort_unstable_by(|a, b| b.cmp(a));
        for idx in excluded_sorted {
            assignments.remove(idx);
        }
    }

    // Final sort: L4 first, then L3, L2, L1 (by tier descending, then by name).
    assignments.sort_by(|a, b| {
        b.tier
            .cmp(&a.tier)
            .then_with(|| a.hops.cmp(&b.hops))
            .then_with(|| a.name.cmp(&b.name))
    });

    TierAllocation {
        focus_node: focus_node_id.to_string(),
        assignments,
        total_tokens: total,
        budget,
        excluded_count,
        downgraded_count,
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "../../tests/unit/analysis/tier_allocator/tier_allocator_test.rs"]
mod tests;
