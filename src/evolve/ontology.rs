//! OntologyStore: persists the evolve graph to a YAML file on disk.

use anyhow::Result;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evolve::{Edge, EdgeType, Node, NodeLayer};

    #[test]
    fn test_ontology_roundtrip() {
        // Use a tempdir: writing to a relative `.selfware/` path here would
        // clobber the user's real ontology file in the test process's CWD.
        let dir = tempfile::tempdir().unwrap();
        let store = OntologyStore::new(dir.path().join("evolve-graph.yaml"));
        let graph = Graph { nodes: vec![], edges: vec![] };
        store.save(&graph).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.nodes.len(), 0);
    }

    #[test]
    fn test_roundtrip_preserves_nodes_and_edges() {
        let dir = tempfile::tempdir().unwrap();
        let store = OntologyStore::new(dir.path().join("graph.yaml"));

        let mut node = Node::code("agent", "src/agent");
        node.tokens = 1234;
        node.lines = 42;
        node.files = 7;
        node.coverage = Some(0.75);
        node.dead_code_ratio = Some(0.1);
        node.warning_count = Some(3);
        node.complexity = Some(2.5);

        let graph = Graph {
            nodes: vec![node],
            edges: vec![Edge {
                from: "agent".to_string(),
                to: "config".to_string(),
                edge_type: EdgeType::DependsOn,
            }],
        };

        store.save(&graph).unwrap();
        let loaded = store.load().unwrap();

        assert_eq!(loaded.nodes.len(), 1);
        let loaded_node = &loaded.nodes[0];
        assert_eq!(loaded_node.id, "agent");
        assert_eq!(loaded_node.layer, NodeLayer::Code);
        assert_eq!(loaded_node.path, Some("src/agent".to_string()));
        assert_eq!(loaded_node.tokens, 1234);
        assert_eq!(loaded_node.lines, 42);
        assert_eq!(loaded_node.files, 7);
        assert_eq!(loaded_node.coverage, Some(0.75));
        assert_eq!(loaded_node.dead_code_ratio, Some(0.1));
        assert_eq!(loaded_node.warning_count, Some(3));
        assert_eq!(loaded_node.complexity, Some(2.5));

        assert_eq!(loaded.edges.len(), 1);
        assert_eq!(loaded.edges[0].from, "agent");
        assert_eq!(loaded.edges[0].to, "config");
        assert_eq!(loaded.edges[0].edge_type, EdgeType::DependsOn);
    }

    #[test]
    fn test_save_creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let store = OntologyStore::new(dir.path().join("nested/dir/graph.yaml"));
        let graph = Graph::default();
        store.save(&graph).unwrap();
        assert!(dir.path().join("nested/dir/graph.yaml").exists());
    }

    #[test]
    fn test_load_missing_file_errors() {
        let dir = tempfile::tempdir().unwrap();
        let store = OntologyStore::new(dir.path().join("missing.yaml"));
        assert!(store.load().is_err());
    }
}
