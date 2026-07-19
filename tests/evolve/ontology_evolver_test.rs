use selfware::evolve::ontology_evolver::{
    OntologyEvolver, OntologyOperation, OntologyProposal,
};
use selfware::evolve::{Edge, EdgeType, Graph, Node, NodeLayer, OntologyStore};

fn evolver_in(dir: &tempfile::TempDir) -> OntologyEvolver {
    OntologyEvolver::new(dir.path().join("evolve-graph.yaml"))
}

fn seed_graph(dir: &tempfile::TempDir, graph: &Graph) {
    OntologyStore::new(dir.path().join("evolve-graph.yaml"))
        .save(graph)
        .unwrap();
}

#[test]
fn test_ontology_evolver_proposes_concept_node() {
    let dir = tempfile::tempdir().unwrap();
    let evolver = evolver_in(&dir);
    let proposal = OntologyProposal::AddConcept {
        name: "safety-layer".to_string(),
        description: "All safety-related code".to_string(),
    };
    let version = evolver.propose_change(proposal).unwrap();
    assert_eq!(version.operations.len(), 1);
}

#[test]
fn test_propose_merge_rewires_edges_and_removes_node() {
    let dir = tempfile::tempdir().unwrap();
    seed_graph(
        &dir,
        &Graph {
            nodes: vec![
                Node::code("a", "src/a"),
                Node::code("b", "src/b"),
                Node::code("c", "src/c"),
            ],
            edges: vec![
                Edge {
                    from: "c".to_string(),
                    to: "a".to_string(),
                    edge_type: EdgeType::DependsOn,
                },
                Edge {
                    from: "b".to_string(),
                    to: "c".to_string(),
                    edge_type: EdgeType::DependsOn,
                },
            ],
        },
    );
    let evolver = evolver_in(&dir);
    let version = evolver
        .propose_change(OntologyProposal::MergeConcepts {
            from: "a".to_string(),
            into: "b".to_string(),
        })
        .unwrap();
    // Merging must produce real operations, not an empty version.
    assert!(version
        .operations
        .contains(&OntologyOperation::RemoveNode { id: "a".to_string() }));
    assert!(version
        .operations
        .iter()
        .any(|op| matches!(op, OntologyOperation::AddEdge { from, to } if from == "c" && to == "b")));
}

#[test]
fn test_propose_split_adds_new_concept_node() {
    let dir = tempfile::tempdir().unwrap();
    let evolver = evolver_in(&dir);
    let version = evolver
        .propose_change(OntologyProposal::SplitConcept {
            concept: "agent".to_string(),
            new_name: "agent-planning".to_string(),
        })
        .unwrap();
    assert_eq!(
        version.operations,
        vec![OntologyOperation::AddNode {
            layer: "concept".to_string(),
            id: "agent-planning".to_string(),
        }]
    );
}

#[test]
fn test_apply_change_persists_merge_to_ontology_file() {
    let dir = tempfile::tempdir().unwrap();
    seed_graph(
        &dir,
        &Graph {
            nodes: vec![Node::code("a", "src/a"), Node::code("b", "src/b")],
            edges: vec![Edge {
                from: "a".to_string(),
                to: "b".to_string(),
                edge_type: EdgeType::DependsOn,
            }],
        },
    );
    let evolver = evolver_in(&dir);
    let version = evolver
        .propose_change(OntologyProposal::MergeConcepts {
            from: "a".to_string(),
            into: "b".to_string(),
        })
        .unwrap();
    evolver.apply_change(version).unwrap();

    let loaded = OntologyStore::new(dir.path().join("evolve-graph.yaml"))
        .load()
        .unwrap();
    assert!(!loaded.nodes.iter().any(|n| n.id == "a"));
    assert!(loaded.nodes.iter().any(|n| n.id == "b"));
    // The a→b edge collapses to a self-loop during merge and is dropped.
    assert!(loaded.edges.is_empty());
}

#[test]
fn test_apply_change_add_concept_creates_file() {
    let dir = tempfile::tempdir().unwrap();
    let evolver = evolver_in(&dir);
    let version = evolver
        .propose_change(OntologyProposal::AddConcept {
            name: "observability".to_string(),
            description: "Tracing and metrics".to_string(),
        })
        .unwrap();
    evolver.apply_change(version).unwrap();

    let loaded = OntologyStore::new(dir.path().join("evolve-graph.yaml"))
        .load()
        .unwrap();
    let node = loaded
        .nodes
        .iter()
        .find(|n| n.id == "observability")
        .unwrap();
    assert_eq!(node.layer, NodeLayer::Concept);
}
