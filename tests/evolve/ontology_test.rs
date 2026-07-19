use selfware::evolve::ontology::{validate_graph, OntologyStore};
use selfware::evolve::{Edge, EdgeType, Graph, Node, NodeLayer};

use crate::edge;

#[test]
fn test_ontology_roundtrip() {
    // Use a tempdir: writing to a relative `.selfware/` path here would
    // clobber the user's real ontology file in the test process's CWD.
    let dir = tempfile::tempdir().unwrap();
    let store = OntologyStore::new(dir.path().join("evolve-graph.yaml"));
    let graph = Graph {
        nodes: vec![],
        edges: vec![],
    };
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

#[test]
fn test_validate_clean_graph_is_valid() {
    let graph = Graph {
        nodes: vec![Node::code("a", "src/a.rs"), Node::code("b", "src/b.rs")],
        edges: vec![edge("a", "b")],
    };
    let report = validate_graph(&graph);
    assert!(report.valid);
    assert!(report.cycles.is_empty());
    assert!(report.dangling_edges.is_empty());
    assert!(report.isolated_nodes.is_empty());
}

#[test]
fn test_validate_detects_cycle() {
    let graph = Graph {
        nodes: vec![
            Node::code("a", "src/a.rs"),
            Node::code("b", "src/b.rs"),
            Node::code("c", "src/c.rs"),
        ],
        edges: vec![edge("a", "b"), edge("b", "c"), edge("c", "a")],
    };
    let report = validate_graph(&graph);
    assert!(!report.valid);
    assert_eq!(report.cycles.len(), 1);
    let cycle = &report.cycles[0];
    assert_eq!(cycle.first(), cycle.last());
    assert_eq!(cycle.len(), 4);
}

#[test]
fn test_validate_detects_dangling_edges() {
    let graph = Graph {
        nodes: vec![Node::code("a", "src/a.rs")],
        edges: vec![edge("a", "ghost")],
    };
    let report = validate_graph(&graph);
    assert!(!report.valid);
    assert_eq!(report.dangling_edges.len(), 1);
    assert_eq!(report.dangling_edges[0].to, "ghost");
}

#[test]
fn test_validate_detects_isolated_nodes() {
    let graph = Graph {
        nodes: vec![
            Node::code("a", "src/a.rs"),
            Node::code("b", "src/b.rs"),
            Node::code("lonely", "src/lonely.rs"),
        ],
        edges: vec![edge("a", "b")],
    };
    let report = validate_graph(&graph);
    assert!(!report.valid);
    assert_eq!(report.isolated_nodes, vec!["lonely".to_string()]);
}

#[test]
fn test_validate_empty_graph_is_valid() {
    assert!(validate_graph(&Graph::default()).valid);
}
