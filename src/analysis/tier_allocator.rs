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
        indices_by_distance.sort_by(|&a, &b| {
            assignments[b].hops.cmp(&assignments[a].hops)
        });

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
        indices_by_distance.sort_by(|&a, &b| {
            assignments[b].hops.cmp(&assignments[a].hops)
        });

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
        b.tier.cmp(&a.tier).then_with(|| a.hops.cmp(&b.hops)).then_with(|| a.name.cmp(&b.name))
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
mod tests {
    use super::*;
    use crate::analysis::code_graph::{CodeGraph, GraphNode, NodeType, EdgeType};

    // ── Test Helpers ────────────────────────────────────────────────────────

    /// Build a small DAG representing a typical module structure:
    ///
    /// ```text
    /// cli -> agent -> tools -> file
    ///                       -> git
    ///            -> config
    /// ```
    fn build_test_graph() -> CodeGraph {
        let mut g = CodeGraph::new("test");

        g.add_node(GraphNode::new("cli", NodeType::Module).in_file("src/cli/mod.rs"));
        g.add_node(GraphNode::new("agent", NodeType::Module).in_file("src/agent/mod.rs"));
        g.add_node(GraphNode::new("tools", NodeType::Module).in_file("src/tools/mod.rs"));
        g.add_node(GraphNode::new("config", NodeType::Module).in_file("src/config/mod.rs"));
        g.add_node(GraphNode::new("file", NodeType::Module).in_file("src/tools/file.rs"));
        g.add_node(GraphNode::new("git", NodeType::Module).in_file("src/tools/git.rs"));

        // cli -> agent -> tools -> file
        //                       -> git
        //            -> config
        g.connect("cli", "agent", EdgeType::Imports);
        g.connect("agent", "tools", EdgeType::Imports);
        g.connect("agent", "config", EdgeType::Imports);
        g.connect("tools", "file", EdgeType::Contains);
        g.connect("tools", "git", EdgeType::Contains);

        g
    }

    #[test]
    fn test_focus_on_agent_module() {
        let graph = build_test_graph();
        let config = TierAllocatorConfig::for_context_window(131072);

        // Focus on "agent" — should be L4
        // Resolve the node ID for "agent"
        let agent_node = graph.get_node("agent").unwrap();
        let alloc = allocate_tiers(&graph, &agent_node.id, &config);

        assert_eq!(alloc.focus_node, agent_node.id);

        // Agent is L4 (focus, 0 hops)
        let agent_assign = alloc.for_node(&agent_node.id).unwrap();
        assert_eq!(agent_assign.tier, ContextTier::Edit);
        assert_eq!(agent_assign.hops, 0);

        // tools and config are L3 (1 hop)
        let tools_node = graph.get_node("tools").unwrap();
        let tools_assign = alloc.for_node(&tools_node.id).unwrap();
        assert_eq!(tools_assign.tier, ContextTier::Integrate);

        let config_node = graph.get_node("config").unwrap();
        let config_assign = alloc.for_node(&config_node.id).unwrap();
        assert_eq!(config_assign.tier, ContextTier::Integrate);

        // cli is L3 (1 hop — cli depends on agent, so agent's dependent)
        let cli_node = graph.get_node("cli").unwrap();
        let cli_assign = alloc.for_node(&cli_node.id).unwrap();
        assert_eq!(cli_assign.tier, ContextTier::Integrate);

        // file and git are L2 (2 hops through tools)
        let file_node = graph.get_node("file").unwrap();
        let file_assign = alloc.for_node(&file_node.id).unwrap();
        assert_eq!(file_assign.tier, ContextTier::Work);

        let git_node = graph.get_node("git").unwrap();
        let git_assign = alloc.for_node(&git_node.id).unwrap();
        assert_eq!(git_assign.tier, ContextTier::Work);
    }

    #[test]
    fn test_focus_on_leaf_node() {
        let graph = build_test_graph();
        let config = TierAllocatorConfig::for_context_window(131072);

        // Focus on "file" (leaf node)
        let file_node = graph.get_node("file").unwrap();
        let alloc = allocate_tiers(&graph, &file_node.id, &config);

        let file_assign = alloc.for_node(&file_node.id).unwrap();
        assert_eq!(file_assign.tier, ContextTier::Edit);
        assert_eq!(file_assign.hops, 0);

        // tools is 1 hop (parent)
        let tools_node = graph.get_node("tools").unwrap();
        let tools_assign = alloc.for_node(&tools_node.id).unwrap();
        assert_eq!(tools_assign.tier, ContextTier::Integrate);

        // agent is 2 hops
        let agent_node = graph.get_node("agent").unwrap();
        let agent_assign = alloc.for_node(&agent_node.id).unwrap();
        assert_eq!(agent_assign.tier, ContextTier::Work);

        // cli is 3 hops -> L1
        let cli_node = graph.get_node("cli").unwrap();
        let cli_assign = alloc.for_node(&cli_node.id).unwrap();
        assert_eq!(cli_assign.tier, ContextTier::Describe);
    }

    #[test]
    fn test_tiny_budget_downgrades_and_excludes() {
        let graph = build_test_graph();

        // Tiny budget: only 1000 tokens for content
        // system_reserve=500, output_reserve=500, context_window=2000
        // content_budget = 2000 - 500 - 500 = 1000
        let config = TierAllocatorConfig {
            context_window: 2000,
            system_reserve: 500,
            output_reserve: 500,
            tier_tokens: [50, 200, 400, 800],
        };

        let agent_node = graph.get_node("agent").unwrap();
        let alloc = allocate_tiers(&graph, &agent_node.id, &config);

        // Budget is 1000. Focus node alone is 800.
        // That leaves only 200 for 5 other nodes.
        // Should downgrade and/or exclude distant nodes.
        assert!(alloc.total_tokens <= 1000, "total {} exceeds budget 1000", alloc.total_tokens);

        // Focus node must always be present at L4.
        let agent_assign = alloc.for_node(&agent_node.id).unwrap();
        assert_eq!(agent_assign.tier, ContextTier::Edit);

        // Some nodes should have been excluded or downgraded.
        assert!(
            alloc.excluded_count > 0 || alloc.downgraded_count > 0,
            "expected downgrades or exclusions with tiny budget"
        );
    }

    #[test]
    fn test_9b_model_budget() {
        let graph = build_test_graph();
        let config = TierAllocatorConfig::for_context_window(32768);

        // 32K context: system=2000, output=40%=13107, content=17661
        assert_eq!(config.content_budget(), 32768 - 2000 - 13107);

        let agent_node = graph.get_node("agent").unwrap();
        let alloc = allocate_tiers(&graph, &agent_node.id, &config);

        // With ~17K budget and 6 small nodes, everything should fit easily.
        assert_eq!(alloc.excluded_count, 0);
        assert!(alloc.total_tokens < config.content_budget());
    }

    #[test]
    fn test_nonexistent_focus_node() {
        let graph = build_test_graph();
        let config = TierAllocatorConfig::default();

        let alloc = allocate_tiers(&graph, "nonexistent_id", &config);
        assert!(alloc.assignments.is_empty());
        assert_eq!(alloc.total_tokens, 0);
    }

    #[test]
    fn test_single_node_graph() {
        let mut graph = CodeGraph::new("single");
        graph.add_node(GraphNode::new("lonely", NodeType::File).in_file("src/lonely.rs"));

        let config = TierAllocatorConfig::default();
        let node = graph.get_node("lonely").unwrap();
        let alloc = allocate_tiers(&graph, &node.id, &config);

        assert_eq!(alloc.assignments.len(), 1);
        assert_eq!(alloc.assignments[0].tier, ContextTier::Edit);
        assert_eq!(alloc.assignments[0].hops, 0);
    }

    #[test]
    fn test_summary_format() {
        let graph = build_test_graph();
        let config = TierAllocatorConfig::default();

        let agent_node = graph.get_node("agent").unwrap();
        let alloc = allocate_tiers(&graph, &agent_node.id, &config);

        let summary = alloc.summary();
        assert!(summary.contains("L4:1"), "expected 1 L4 node: {}", summary);
        assert!(summary.contains("L3:"), "expected L3 count: {}", summary);
        assert!(summary.contains("tokens"), "expected token info: {}", summary);
    }

    #[test]
    fn test_tier_ordering() {
        let graph = build_test_graph();
        let config = TierAllocatorConfig::default();

        let agent_node = graph.get_node("agent").unwrap();
        let alloc = allocate_tiers(&graph, &agent_node.id, &config);

        let tiers: Vec<ContextTier> = alloc.assignments.iter().map(|a| a.tier).collect();
        for window in tiers.windows(2) {
            assert!(
                window[0] >= window[1],
                "assignments not sorted by tier: {:?} before {:?}",
                window[0], window[1]
            );
        }
    }

    // ── ContextTier unit tests ──────────────────────────────────────────────

    #[test]
    fn test_tier_default_tokens() {
        assert_eq!(ContextTier::Describe.default_tokens(), 50);
        assert_eq!(ContextTier::Work.default_tokens(), 200);
        assert_eq!(ContextTier::Integrate.default_tokens(), 400);
        assert_eq!(ContextTier::Edit.default_tokens(), 800);
    }

    #[test]
    fn test_tier_from_hops() {
        assert_eq!(ContextTier::from_hops(0), ContextTier::Edit);
        assert_eq!(ContextTier::from_hops(1), ContextTier::Integrate);
        assert_eq!(ContextTier::from_hops(2), ContextTier::Work);
        assert_eq!(ContextTier::from_hops(3), ContextTier::Describe);
        assert_eq!(ContextTier::from_hops(100), ContextTier::Describe);
    }

    #[test]
    fn test_tier_as_str() {
        assert_eq!(ContextTier::Edit.as_str(), "L4-Edit");
        assert_eq!(ContextTier::Integrate.as_str(), "L3-Integrate");
        assert_eq!(ContextTier::Work.as_str(), "L2-Work");
        assert_eq!(ContextTier::Describe.as_str(), "L1-Describe");
    }

    #[test]
    fn test_tier_display() {
        assert_eq!(format!("{}", ContextTier::Edit), "L4-Edit");
        assert_eq!(format!("{}", ContextTier::Describe), "L1-Describe");
    }

    #[test]
    fn test_tier_ord() {
        assert!(ContextTier::Edit > ContextTier::Integrate);
        assert!(ContextTier::Integrate > ContextTier::Work);
        assert!(ContextTier::Work > ContextTier::Describe);
    }

    // ── TierAllocatorConfig tests ───────────────────────────────────────────

    #[test]
    fn test_config_9b_model() {
        let config = TierAllocatorConfig::for_context_window(32768);
        assert_eq!(config.system_reserve, 2000);
        assert_eq!(config.output_reserve, (32768.0 * 0.40) as usize);
        assert!(config.content_budget() > 0);
        assert!(config.content_budget() < 32768);
    }

    #[test]
    fn test_config_27b_model() {
        let config = TierAllocatorConfig::for_context_window(131072);
        assert_eq!(config.system_reserve, 4000);
        assert_eq!(config.output_reserve, (131072.0 * 0.30) as usize);
        assert!(config.content_budget() > config.system_reserve);
    }

    #[test]
    fn test_config_tokens_for_tier() {
        let config = TierAllocatorConfig::default();
        assert_eq!(config.tokens_for_tier(ContextTier::Describe), 50);
        assert_eq!(config.tokens_for_tier(ContextTier::Work), 200);
        assert_eq!(config.tokens_for_tier(ContextTier::Integrate), 400);
        assert_eq!(config.tokens_for_tier(ContextTier::Edit), 800);
    }

    #[test]
    fn test_config_custom_tier_tokens() {
        let config = TierAllocatorConfig {
            context_window: 65536,
            system_reserve: 1000,
            output_reserve: 10000,
            tier_tokens: [10, 100, 300, 500],
        };
        assert_eq!(config.tokens_for_tier(ContextTier::Describe), 10);
        assert_eq!(config.tokens_for_tier(ContextTier::Edit), 500);
        assert_eq!(config.content_budget(), 65536 - 1000 - 10000);
    }

    #[test]
    fn test_config_zero_context_window() {
        let config = TierAllocatorConfig::for_context_window(0);
        assert_eq!(config.content_budget(), 0);
    }

    // ── TierAllocation query tests ──────────────────────────────────────────

    #[test]
    fn test_allocation_at_tier() {
        let graph = build_test_graph();
        let config = TierAllocatorConfig::default();
        let agent_node = graph.get_node("agent").unwrap();
        let alloc = allocate_tiers(&graph, &agent_node.id, &config);

        let l4 = alloc.at_tier(ContextTier::Edit);
        assert_eq!(l4.len(), 1);
        assert_eq!(l4[0].name, "agent");

        let l3 = alloc.at_tier(ContextTier::Integrate);
        assert!(l3.len() >= 2, "expected at least tools + config at L3");
    }

    #[test]
    fn test_allocation_for_node_miss() {
        let graph = build_test_graph();
        let config = TierAllocatorConfig::default();
        let agent_node = graph.get_node("agent").unwrap();
        let alloc = allocate_tiers(&graph, &agent_node.id, &config);

        assert!(alloc.for_node("nonexistent_id_xyz").is_none());
    }

    #[test]
    fn test_allocation_utilization_pct() {
        let graph = build_test_graph();
        let config = TierAllocatorConfig::default();
        let agent_node = graph.get_node("agent").unwrap();
        let alloc = allocate_tiers(&graph, &agent_node.id, &config);

        let pct = alloc.utilization_pct();
        assert!(pct >= 0.0 && pct <= 100.0, "utilization {} out of range", pct);
    }

    #[test]
    fn test_allocation_utilization_zero_budget() {
        let alloc = TierAllocation {
            focus_node: "x".into(),
            assignments: vec![],
            total_tokens: 0,
            budget: 0,
            excluded_count: 0,
            downgraded_count: 0,
        };
        assert_eq!(alloc.utilization_pct(), 0.0);
    }

    // ── BFS tests ───────────────────────────────────────────────────────────

    #[test]
    fn test_bfs_bidirectional() {
        // BFS should follow both outgoing AND incoming edges
        let mut graph = CodeGraph::new("bidi");
        graph.add_node(GraphNode::new("a", NodeType::File));
        graph.add_node(GraphNode::new("b", NodeType::File));
        graph.add_node(GraphNode::new("c", NodeType::File));
        // a -> b -> c (a depends on b, b depends on c)
        graph.connect("a", "b", EdgeType::Imports);
        graph.connect("b", "c", EdgeType::Imports);

        let b_node = graph.get_node("b").unwrap();
        let distances = bfs_hop_distances(&graph, &b_node.id);

        // From b: a is 1 hop (incoming), c is 1 hop (outgoing)
        let a_node = graph.get_node("a").unwrap();
        let c_node = graph.get_node("c").unwrap();
        assert_eq!(distances[&b_node.id], 0);
        assert_eq!(distances[&a_node.id], 1);
        assert_eq!(distances[&c_node.id], 1);
    }

    #[test]
    fn test_bfs_disconnected_component() {
        let mut graph = CodeGraph::new("disconn");
        graph.add_node(GraphNode::new("a", NodeType::File));
        graph.add_node(GraphNode::new("b", NodeType::File));
        graph.add_node(GraphNode::new("island", NodeType::File));
        graph.connect("a", "b", EdgeType::Imports);
        // "island" has no edges

        let a_node = graph.get_node("a").unwrap();
        let distances = bfs_hop_distances(&graph, &a_node.id);

        assert!(distances.contains_key(&a_node.id));
        let b_node = graph.get_node("b").unwrap();
        assert!(distances.contains_key(&b_node.id));
        let island_node = graph.get_node("island").unwrap();
        assert!(!distances.contains_key(&island_node.id), "island should be unreachable");
    }

    // ── Deep DAG tests ──────────────────────────────────────────────────────

    #[test]
    fn test_deep_chain_tier_assignment() {
        // a -> b -> c -> d -> e (linear chain, 4 hops from a to e)
        let mut graph = CodeGraph::new("chain");
        for name in &["a", "b", "c", "d", "e"] {
            graph.add_node(GraphNode::new(name, NodeType::File).in_file(&format!("src/{}.rs", name)));
        }
        graph.connect("a", "b", EdgeType::Imports);
        graph.connect("b", "c", EdgeType::Imports);
        graph.connect("c", "d", EdgeType::Imports);
        graph.connect("d", "e", EdgeType::Imports);

        let config = TierAllocatorConfig::default();
        let a_node = graph.get_node("a").unwrap();
        let alloc = allocate_tiers(&graph, &a_node.id, &config);

        // a=L4(0), b=L3(1), c=L2(2), d=L1(3), e=L1(4)
        let b_node = graph.get_node("b").unwrap();
        let c_node = graph.get_node("c").unwrap();
        let d_node = graph.get_node("d").unwrap();
        let e_node = graph.get_node("e").unwrap();

        assert_eq!(alloc.for_node(&a_node.id).unwrap().tier, ContextTier::Edit);
        assert_eq!(alloc.for_node(&b_node.id).unwrap().tier, ContextTier::Integrate);
        assert_eq!(alloc.for_node(&c_node.id).unwrap().tier, ContextTier::Work);
        assert_eq!(alloc.for_node(&d_node.id).unwrap().tier, ContextTier::Describe);
        assert_eq!(alloc.for_node(&e_node.id).unwrap().tier, ContextTier::Describe);
    }

    #[test]
    fn test_fan_out_graph() {
        // hub -> a, hub -> b, hub -> c, hub -> d (star topology)
        let mut graph = CodeGraph::new("star");
        graph.add_node(GraphNode::new("hub", NodeType::File).in_file("src/hub.rs"));
        for name in &["a", "b", "c", "d"] {
            graph.add_node(GraphNode::new(name, NodeType::File).in_file(&format!("src/{}.rs", name)));
            graph.connect("hub", name, EdgeType::Imports);
        }

        let config = TierAllocatorConfig::default();
        let hub_node = graph.get_node("hub").unwrap();
        let alloc = allocate_tiers(&graph, &hub_node.id, &config);

        // hub=L4, all children=L3 (1 hop)
        assert_eq!(alloc.for_node(&hub_node.id).unwrap().tier, ContextTier::Edit);
        for name in &["a", "b", "c", "d"] {
            let node = graph.get_node(name).unwrap();
            assert_eq!(alloc.for_node(&node.id).unwrap().tier, ContextTier::Integrate);
        }
    }

    // ── Budget pressure tests ───────────────────────────────────────────────

    #[test]
    fn test_budget_exactly_fits() {
        // Build graph where total exactly equals budget
        let mut graph = CodeGraph::new("exact");
        graph.add_node(GraphNode::new("focus", NodeType::File));
        graph.add_node(GraphNode::new("neighbor", NodeType::File));
        graph.connect("focus", "neighbor", EdgeType::Imports);

        // focus=800 + neighbor=400 = 1200
        let config = TierAllocatorConfig {
            context_window: 2200,
            system_reserve: 500,
            output_reserve: 500,
            tier_tokens: [50, 200, 400, 800],
        };
        // content_budget = 1200

        let focus = graph.get_node("focus").unwrap();
        let alloc = allocate_tiers(&graph, &focus.id, &config);

        assert_eq!(alloc.total_tokens, 1200);
        assert_eq!(alloc.excluded_count, 0);
        assert_eq!(alloc.downgraded_count, 0);
    }

    #[test]
    fn test_budget_one_token_over_triggers_downgrade() {
        let mut graph = CodeGraph::new("over");
        graph.add_node(GraphNode::new("focus", NodeType::File));
        graph.add_node(GraphNode::new("neighbor", NodeType::File));
        graph.connect("focus", "neighbor", EdgeType::Imports);

        // focus=800 + neighbor=400 = 1200, but budget is 1199
        let config = TierAllocatorConfig {
            context_window: 2199,
            system_reserve: 500,
            output_reserve: 500,
            tier_tokens: [50, 200, 400, 800],
        };

        let focus = graph.get_node("focus").unwrap();
        let alloc = allocate_tiers(&graph, &focus.id, &config);

        assert!(alloc.total_tokens <= 1199);
        assert!(alloc.downgraded_count > 0 || alloc.excluded_count > 0);
        // Focus must remain L4
        assert_eq!(alloc.for_node(&focus.id).unwrap().tier, ContextTier::Edit);
    }

    #[test]
    fn test_budget_forces_exclusion_after_all_downgraded() {
        // Many nodes, all at L1 already, budget too small even for L1
        let mut graph = CodeGraph::new("exclude");
        graph.add_node(GraphNode::new("focus", NodeType::File));
        for i in 0..50 {
            let name = format!("node_{}", i);
            graph.add_node(GraphNode::new(&name, NodeType::File));
            graph.connect("focus", &name, EdgeType::Imports);
        }

        // focus=800 + 50 neighbors at best L1=50 each = 800+2500=3300
        // Budget: only 1000
        let config = TierAllocatorConfig {
            context_window: 2000,
            system_reserve: 500,
            output_reserve: 500,
            tier_tokens: [50, 200, 400, 800],
        };

        let focus = graph.get_node("focus").unwrap();
        let alloc = allocate_tiers(&graph, &focus.id, &config);

        assert!(alloc.total_tokens <= 1000);
        assert!(alloc.excluded_count > 0, "should have excluded some nodes");
        assert_eq!(alloc.for_node(&focus.id).unwrap().tier, ContextTier::Edit);
    }

    // ── Edge type and metadata tests ────────────────────────────────────────

    #[test]
    fn test_file_path_propagated() {
        let mut graph = CodeGraph::new("paths");
        graph.add_node(GraphNode::new("main", NodeType::File).in_file("src/main.rs"));
        graph.add_node(GraphNode::new("lib", NodeType::File).in_file("src/lib.rs"));
        graph.connect("main", "lib", EdgeType::Imports);

        let config = TierAllocatorConfig::default();
        let main_node = graph.get_node("main").unwrap();
        let alloc = allocate_tiers(&graph, &main_node.id, &config);

        let main_assign = alloc.for_node(&main_node.id).unwrap();
        assert_eq!(main_assign.file_path.as_deref(), Some("src/main.rs"));

        let lib_node = graph.get_node("lib").unwrap();
        let lib_assign = alloc.for_node(&lib_node.id).unwrap();
        assert_eq!(lib_assign.file_path.as_deref(), Some("src/lib.rs"));
    }

    #[test]
    fn test_node_without_file_path() {
        let mut graph = CodeGraph::new("no_path");
        graph.add_node(GraphNode::new("abstract_module", NodeType::Module));

        let config = TierAllocatorConfig::default();
        let node = graph.get_node("abstract_module").unwrap();
        let alloc = allocate_tiers(&graph, &node.id, &config);

        assert_eq!(alloc.assignments[0].file_path, None);
    }

    #[test]
    fn test_multiple_edge_types() {
        let mut graph = CodeGraph::new("multi_edge");
        graph.add_node(GraphNode::new("a", NodeType::File));
        graph.add_node(GraphNode::new("b", NodeType::File));
        graph.add_node(GraphNode::new("c", NodeType::File));
        // a imports b, a contains c (different edge types, same hop distance)
        graph.connect("a", "b", EdgeType::Imports);
        graph.connect("a", "c", EdgeType::Contains);

        let config = TierAllocatorConfig::default();
        let a = graph.get_node("a").unwrap();
        let alloc = allocate_tiers(&graph, &a.id, &config);

        let b = graph.get_node("b").unwrap();
        let c = graph.get_node("c").unwrap();
        // Both should be L3 (1 hop regardless of edge type)
        assert_eq!(alloc.for_node(&b.id).unwrap().tier, ContextTier::Integrate);
        assert_eq!(alloc.for_node(&c.id).unwrap().tier, ContextTier::Integrate);
    }

    // ── Summary format tests ────────────────────────────────────────────────

    #[test]
    fn test_summary_contains_all_fields() {
        let graph = build_test_graph();
        let config = TierAllocatorConfig::default();
        let agent_node = graph.get_node("agent").unwrap();
        let alloc = allocate_tiers(&graph, &agent_node.id, &config);

        let s = alloc.summary();
        assert!(s.contains("Focus:"), "missing Focus: {}", s);
        assert!(s.contains("L4:"), "missing L4: {}", s);
        assert!(s.contains("L3:"), "missing L3: {}", s);
        assert!(s.contains("L2:"), "missing L2: {}", s);
        assert!(s.contains("L1:"), "missing L1: {}", s);
        assert!(s.contains("tokens"), "missing tokens: {}", s);
        assert!(s.contains("excluded:"), "missing excluded: {}", s);
        assert!(s.contains("downgraded:"), "missing downgraded: {}", s);
    }

    #[test]
    fn test_empty_allocation_summary() {
        let alloc = TierAllocation {
            focus_node: "gone".into(),
            assignments: vec![],
            total_tokens: 0,
            budget: 100,
            excluded_count: 0,
            downgraded_count: 0,
        };
        let s = alloc.summary();
        assert!(s.contains("L4:0"));
        assert!(s.contains("0/100 tokens"));
    }
}
