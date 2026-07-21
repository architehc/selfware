//! OntologyStore: persists the evolve graph to a YAML file on disk, and
//! validates the graph's structural integrity.

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;

use super::{EdgeType, Graph};

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
        let parent = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        std::fs::create_dir_all(parent)?;
        if std::fs::symlink_metadata(&self.path)
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            anyhow::bail!("refusing to replace symlinked ontology store");
        }
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(yaml.as_bytes())?;
        temporary.flush()?;
        temporary.as_file().sync_all()?;
        temporary.persist(&self.path).map_err(|error| {
            anyhow::anyhow!(
                "failed to atomically persist {}: {}",
                self.path.display(),
                error.error
            )
        })?;
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
    /// Duplicate logical node IDs, sorted lexicographically.
    pub duplicate_ids: Vec<String>,
    /// Each non-dependency cycle is reported as the node-id path that closes
    /// on itself. Dependency cycles are valid code-graph relationships.
    pub cycles: Vec<Vec<String>>,
    pub dangling_edges: Vec<DanglingEdge>,
    pub isolated_nodes: Vec<String>,
}

/// Checks the graph for duplicate IDs, hierarchy cycles, dangling edges, and
/// isolated nodes. `DependsOn` cycles are intentionally allowed.
pub fn validate_graph(graph: &Graph) -> ValidationReport {
    let mut node_counts: HashMap<&str, usize> = HashMap::new();
    for node in &graph.nodes {
        *node_counts.entry(node.id.as_str()).or_default() += 1;
    }
    let node_ids: HashSet<&str> = node_counts.keys().copied().collect();

    let mut report = ValidationReport::default();
    report.duplicate_ids = node_counts
        .into_iter()
        .filter_map(|(id, count)| (count > 1).then(|| id.to_string()))
        .collect();
    report.duplicate_ids.sort();

    for edge in &graph.edges {
        if !node_ids.contains(edge.from.as_str()) || !node_ids.contains(edge.to.as_str()) {
            report.dangling_edges.push(DanglingEdge {
                from: edge.from.clone(),
                to: edge.to.clone(),
            });
        }
    }
    report
        .dangling_edges
        .sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));

    let mut connected: HashSet<&str> = HashSet::new();
    for edge in &graph.edges {
        if node_ids.contains(edge.from.as_str()) && node_ids.contains(edge.to.as_str()) {
            connected.insert(edge.from.as_str());
            connected.insert(edge.to.as_str());
        }
    }
    report.isolated_nodes = node_ids
        .iter()
        .filter(|id| !connected.contains(**id))
        .map(|id| (*id).to_string())
        .collect();
    report.isolated_nodes.sort();

    // Only ontology relationships participate in cycle validation. Rust code
    // dependencies commonly cycle and remain useful graph facts.
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &graph.edges {
        if edge.edge_type != EdgeType::DependsOn
            && node_ids.contains(edge.from.as_str())
            && node_ids.contains(edge.to.as_str())
        {
            adjacency
                .entry(edge.from.as_str())
                .or_default()
                .push(edge.to.as_str());
        }
    }
    for children in adjacency.values_mut() {
        children.sort_unstable();
        children.dedup();
    }

    const WHITE: u8 = 0;
    const GRAY: u8 = 1;
    const BLACK: u8 = 2;
    let mut color: HashMap<&str, u8> = node_ids.iter().map(|id| (*id, WHITE)).collect();
    let mut starts: Vec<&str> = node_ids.iter().copied().collect();
    starts.sort_unstable();

    for start in starts {
        if color[start] != WHITE {
            continue;
        }
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
                    if let Some(position) = path.iter().position(|node| *node == next) {
                        let mut cycle: Vec<String> = path[position..]
                            .iter()
                            .map(|node| node.to_string())
                            .collect();
                        cycle.push(next.to_string());
                        report.cycles.push(cycle);
                    }
                }
                _ => {}
            }
        }
    }

    report.cycles.sort();
    report.cycles.dedup();
    report.valid = report.duplicate_ids.is_empty()
        && report.cycles.is_empty()
        && report.dangling_edges.is_empty()
        && report.isolated_nodes.is_empty();
    report
}
