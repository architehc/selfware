//! Architectural clustering of the code graph.
//!
//! 45 top-level components is too flat to navigate. Viewed as a local dev-loop
//! (perceive → reason → act → verify → learn), they collapse into 10 clusters.
//! This aggregates the production graph into cluster super-nodes with
//! inter-cluster dependency edges, in the same `{nodes, edges}` shape the D3
//! view already renders — so the graph can default to 10 clusters, expandable.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Serialize;

use super::{EdgeType, Graph, NodeLayer};

/// The 10 clusters, in loop order. Anything unmapped falls to Foundation.
pub fn cluster_of(component: &str) -> &'static str {
    match component {
        "agent" | "orchestration" | "cli" | "input" | "interview" => "Loop Core",
        "api" | "tokens" | "token_count" | "tool_parser" | "llm_doctor" => "Reasoning",
        "tools" | "computer" | "mcp" | "lsp" => "Action",
        "cognitive" | "memory" | "consolidation" | "session" => "Cognition",
        "safety" | "testing" | "self_healing" | "supervision" => "Safety & Verify",
        "evolve" | "evolution" | "swl" | "analysis" => "Evolution",
        "ui" | "output" | "templates" => "Interface",
        "observability" | "doctor" | "devops" => "Observability",
        "bench_harness" | "vlm_bench" | "test_support" | "bin" => "Eval / Bench",
        _ => "Foundation",
    }
}

/// Extract the top-level component from a node id: `crate::agent::execution`
/// → `agent`; `crate` → `crate`; `bin::codegraph` → `bin`.
pub fn component_of(node_id: &str) -> String {
    let s = node_id.strip_prefix("crate::").unwrap_or(node_id);
    s.split("::").next().unwrap_or(s).to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterNode {
    pub id: String,
    pub layer: String,
    pub tokens: usize,
    pub lines: usize,
    pub files: usize,
    pub member_count: usize,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterEdge {
    pub from: String,
    pub to: String,
    pub weight: usize,
    pub edge_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusteredGraph {
    pub nodes: Vec<ClusterNode>,
    pub edges: Vec<ClusterEdge>,
}

#[derive(Default)]
struct Agg {
    tokens: usize,
    lines: usize,
    files: usize,
    components: BTreeSet<String>,
}

/// Aggregate the production (Code-layer) graph into cluster super-nodes and
/// inter-cluster dependency edges.
pub fn clustered(graph: &Graph) -> ClusteredGraph {
    let mut aggs: BTreeMap<&'static str, Agg> = BTreeMap::new();
    let mut node_cluster: HashMap<&str, &'static str> = HashMap::new();

    for node in &graph.nodes {
        if node.layer != NodeLayer::Code {
            continue;
        }
        let component = component_of(&node.id);
        let cluster = cluster_of(&component);
        node_cluster.insert(node.id.as_str(), cluster);
        let agg = aggs.entry(cluster).or_default();
        agg.tokens += node.tokens;
        agg.lines += node.lines;
        agg.files += node.files;
        agg.components.insert(component);
    }

    let mut weights: BTreeMap<(&'static str, &'static str), usize> = BTreeMap::new();
    for edge in &graph.edges {
        if !matches!(edge.edge_type, EdgeType::DependsOn) {
            continue;
        }
        let (Some(&fc), Some(&tc)) = (
            node_cluster.get(edge.from.as_str()),
            node_cluster.get(edge.to.as_str()),
        ) else {
            continue;
        };
        if fc != tc {
            *weights.entry((fc, tc)).or_default() += 1;
        }
    }

    let nodes = aggs
        .into_iter()
        .map(|(cluster, agg)| ClusterNode {
            id: cluster.to_string(),
            layer: "cluster".to_string(),
            tokens: agg.tokens,
            lines: agg.lines,
            files: agg.files,
            member_count: agg.components.len(),
            members: agg.components.into_iter().collect(),
        })
        .collect();

    let edges = weights
        .into_iter()
        .map(|((from, to), weight)| ClusterEdge {
            from: from.to_string(),
            to: to.to_string(),
            weight,
            edge_type: "DependsOn".to_string(),
        })
        .collect();

    ClusteredGraph { nodes, edges }
}
