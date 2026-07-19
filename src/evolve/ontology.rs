//! OntologyStore: persists the evolve graph to a YAML file on disk, and
//! validates the graph's structural integrity (cycles, dangling edges,
//! isolated nodes).

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::Graph;

/// Persists the evolve graph to a YAML file.
pub struct OntologyStore {
    path: PathBuf,
}

impl OntologyStore {
    pub fn new(path: impl AsRef<std::path::Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn save(&self, graph: &Graph) -> Result<()> {
        let yaml = serde_yaml::to_string(graph)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, yaml)?;
        Ok(())
    }

    pub fn load(&self) -> Result<Graph> {
        let yaml = std::fs::read_to_string(&self.path)?;
        Ok(serde_yaml::from_str(&yaml)?)
    }
}

/// An edge whose `from` or `to` endpoint does not exist in the node set.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DanglingEdge {
    pub from: String,
    pub to: String,
}

/// Structural integrity report for a graph.
#[derive(Debug, Default, serde::Serialize)]
pub struct ValidationReport {
    pub valid: bool,
    /// Each cycle is reported as the node-id path that closes on itself.
    pub cycles: Vec<Vec<String>>,
    pub dangling_edges: Vec<DanglingEdge>,
    pub isolated_nodes: Vec<String>,
}

/// Checks the graph for cycles, dangling edges, and isolated nodes.
pub fn validate_graph(graph: &Graph) -> ValidationReport {
    let node_ids: HashSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();

    let mut report = ValidationReport::default();

    // Dangling edges: endpoints that reference unknown nodes.
    for edge in &graph.edges {
        if !node_ids.contains(edge.from.as_str()) || !node_ids.contains(edge.to.as_str()) {
            report.dangling_edges.push(DanglingEdge {
                from: edge.from.clone(),
                to: edge.to.clone(),
            });
        }
    }

    // Isolated nodes: no well-formed edge touches them.
    let mut connected: HashSet<&str> = HashSet::new();
    for edge in &graph.edges {
        if node_ids.contains(edge.from.as_str()) && node_ids.contains(edge.to.as_str()) {
            connected.insert(edge.from.as_str());
            connected.insert(edge.to.as_str());
        }
    }
    for node in &graph.nodes {
        if !connected.contains(node.id.as_str()) {
            report.isolated_nodes.push(node.id.clone());
        }
    }

    // Cycles: iterative DFS with white/gray/black coloring over the
    // adjacency of well-formed edges only.
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &graph.edges {
        if node_ids.contains(edge.from.as_str()) && node_ids.contains(edge.to.as_str()) {
            adjacency.entry(edge.from.as_str()).or_default().push(edge.to.as_str());
        }
    }

    const WHITE: u8 = 0;
    const GRAY: u8 = 1;
    const BLACK: u8 = 2;
    let mut color: HashMap<&str, u8> = node_ids.iter().map(|id| (*id, WHITE)).collect();

    for &start in &node_ids {
        if color[start] != WHITE {
            continue;
        }
        // Stack of (node, next-child-index) frames plus the current DFS path.
        let mut stack: Vec<(&str, usize)> = vec![(start, 0)];
        let mut path: Vec<&str> = vec![start];
        color.insert(start, GRAY);

        while let Some(frame) = stack.last_mut() {
            let node = frame.0;
            let children: &[&str] = adjacency.get(node).map_or(&[], Vec::as_slice);
            if frame.1 >= children.len() {
                color.insert(node, BLACK);
                stack.pop();
                path.pop();
                continue;
            }
            let next = children[frame.1];
            frame.1 += 1;
            match color[next] {
                WHITE => {
                    color.insert(next, GRAY);
                    stack.push((next, 0));
                    path.push(next);
                }
                GRAY => {
                    // Back edge: report the cycle from its first occurrence.
                    if let Some(pos) = path.iter().position(|n| *n == next) {
                        let mut cycle: Vec<String> =
                            path[pos..].iter().map(|s| s.to_string()).collect();
                        cycle.push(next.to_string());
                        report.cycles.push(cycle);
                    }
                }
                _ => {}
            }
        }
    }

    report.valid =
        report.cycles.is_empty() && report.dangling_edges.is_empty() && report.isolated_nodes.is_empty();
    report
}
