//! GraphBuilder: scans `src/` and builds the code dependency graph.

use anyhow::Result;
use std::path::{Path, PathBuf};

use super::dedup::{DeduplicationAnalyzer, DuplicateKind};
use super::quality::QualityAnalyzer;
use super::{Edge, EdgeType, Graph, Node};

pub struct GraphBuilder {
    src_root: PathBuf,
}

impl GraphBuilder {
    pub fn new(src_root: impl AsRef<Path>) -> Self {
        Self {
            src_root: src_root.as_ref().to_path_buf(),
        }
    }

    pub fn scan_src(&self) -> Result<Graph> {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        let analyzer = QualityAnalyzer::new();

        // Scan top-level entries in src/
        for entry in std::fs::read_dir(&self.src_root)? {
            let entry = entry?;
            let path = entry.path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();

            if name == "bin" || name.starts_with('.') {
                continue;
            }

            // File modules (e.g. `errors.rs`) get the bare module name as id
            // so `use crate::errors;` dependencies resolve to an existing node.
            let id = name.strip_suffix(".rs").unwrap_or(&name);

            let mut node = Node::code(id, &format!("src/{}", name));
            self.populate_metrics(&mut node, &path)?;
            if let Err(e) = analyzer.analyze_node(&mut node) {
                eprintln!("quality analysis skipped for {name}: {e}");
            }
            nodes.push(node);
        }

        // Build depends_on edges from `use crate::X` statements. Only emit
        // edges whose target resolves to a real node so the frontend's
        // d3.forceLink never sees a dangling reference.
        for node in &nodes {
            if let Some(ref path) = node.path {
                let deps = self.extract_dependencies(path)?;
                for dep in deps {
                    let target_exists = nodes.iter().any(|n| n.id == dep);
                    if !target_exists {
                        continue;
                    }
                    edges.push(Edge {
                        from: node.id.clone(),
                        to: dep,
                        edge_type: EdgeType::DependsOn,
                    });
                }
            }
        }

        // Flag duplicate clusters: exact matches get DuplicateOf edges,
        // near-duplicates get SimilarTo edges.
        let dedup = DeduplicationAnalyzer::new();
        let mut graph = Graph { nodes, edges };
        match dedup.find_duplicates(&graph) {
            Ok(dupes) => {
                for pair in dupes {
                    let edge_type = match pair.kind {
                        DuplicateKind::Exact => EdgeType::DuplicateOf,
                        DuplicateKind::Near => EdgeType::SimilarTo,
                    };
                    graph.edges.push(Edge {
                        from: pair.first,
                        to: pair.second,
                        edge_type,
                    });
                }
            }
            Err(e) => eprintln!("dedup analysis skipped: {e}"),
        }

        Ok(graph)
    }

    fn populate_metrics(&self, node: &mut Node, path: &Path) -> Result<()> {
        if path.is_file() {
            let content = std::fs::read_to_string(path)?;
            node.lines = content.lines().count();
            node.tokens = content.len() / 4;
            node.files = 1;
        } else {
            let mut total_lines = 0;
            let mut total_bytes = 0;
            let mut file_count = 0;
            for entry in walkdir::WalkDir::new(path) {
                let entry = entry?;
                if entry.path().extension().map_or(false, |e| e == "rs") {
                    let content = std::fs::read_to_string(entry.path())?;
                    total_lines += content.lines().count();
                    total_bytes += content.len();
                    file_count += 1;
                }
            }
            node.lines = total_lines;
            node.tokens = total_bytes / 4;
            node.files = file_count;
        }
        Ok(())
    }

    fn extract_dependencies(&self, path: &str) -> Result<Vec<String>> {
        let mut deps = Vec::new();
        let full_path = self
            .src_root
            .join(path.strip_prefix("src/").unwrap_or(path));
        let targets: Vec<PathBuf> = if full_path.is_file() {
            vec![full_path]
        } else {
            walkdir::WalkDir::new(&full_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map_or(false, |x| x == "rs"))
                .map(|e| e.path().to_path_buf())
                .collect()
        };

        for file in targets {
            let content = std::fs::read_to_string(&file)?;
            for line in content.lines() {
                if let Some(rest) = line.trim_start().strip_prefix("use crate::") {
                    if let Some(first) = rest.split("::").next() {
                        if !first.is_empty() && first != "evolve" {
                            deps.push(first.to_string());
                        }
                    }
                }
            }
        }

        deps.sort();
        deps.dedup();
        Ok(deps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_builder_scans_agent_component() {
        let builder = GraphBuilder::new("src");
        let graph = builder.scan_src().unwrap();
        let agent = graph.nodes.iter().find(|n| n.id == "agent").unwrap();
        assert!(agent.tokens > 0);
        assert!(agent.files > 0);
    }

    #[test]
    fn test_depends_on_edges_never_dangle() {
        let builder = GraphBuilder::new("src");
        let graph = builder.scan_src().unwrap();
        let ids: std::collections::HashSet<&str> =
            graph.nodes.iter().map(|n| n.id.as_str()).collect();
        // File modules must be normalized to bare names (no `.rs` suffix).
        assert!(ids.iter().all(|id| !id.ends_with(".rs")));
        for edge in &graph.edges {
            assert!(ids.contains(edge.from.as_str()), "dangling from: {}", edge.from);
            assert!(ids.contains(edge.to.as_str()), "dangling to: {}", edge.to);
        }
    }
}
