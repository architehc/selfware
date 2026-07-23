use selfware::evolve::deletion::{preview_deletion, require_executable};
use selfware::evolve::{Edge, EdgeType, Graph, IdeEngine, Node};

fn deletion_fixture() -> (tempfile::TempDir, IdeEngine, Graph) {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("src")).unwrap();
    std::fs::write(project.path().join("src/target.rs"), "pub fn target() {}\n").unwrap();
    std::fs::write(
        project.path().join("src/dependent.rs"),
        "use crate::target::target;\n",
    )
    .unwrap();

    let graph = Graph {
        nodes: vec![
            Node::code("crate::dependent", "src/dependent.rs"),
            Node::code("crate::target", "src/target.rs"),
        ],
        edges: vec![
            Edge {
                from: "crate::dependent".to_string(),
                to: "crate::target".to_string(),
                edge_type: EdgeType::DependsOn,
            },
            Edge {
                from: "crate::target".to_string(),
                to: "crate::dependent".to_string(),
                edge_type: EdgeType::Influences,
            },
        ],
    };
    let ide = IdeEngine::for_project(project.path());
    (project, ide, graph)
}

#[test]
fn test_deletion_preview_is_exact_non_mutating_and_blocked() {
    let (project, ide, graph) = deletion_fixture();
    let before = std::fs::read_to_string(project.path().join("src/target.rs")).unwrap();
    let expected_hash = ide.read_document("src/target.rs").unwrap().hash;

    let preview = preview_deletion(&graph, &ide, "crate::target").unwrap();

    assert_eq!(preview.lifecycle, "proposed");
    assert_eq!(preview.action, "preview_source_deletion");
    assert_eq!(preview.target.logical_node_id, "crate::target");
    assert_eq!(preview.target.path, "src/target.rs");
    assert_eq!(preview.target.content_hash, expected_hash);
    assert_eq!(preview.target.graph_revision.len(), 64);
    assert_eq!(preview.removed_lines, 1);
    assert_eq!(preview.inbound.len(), 1);
    assert_eq!(preview.inbound[0].direction, "inbound");
    assert_eq!(preview.inbound[0].from, "crate::dependent");
    assert_eq!(preview.inbound[0].edge_type, "DependsOn");
    assert_eq!(preview.outbound.len(), 1);
    assert_eq!(preview.outbound[0].direction, "outbound");
    assert_eq!(preview.outbound[0].to, "crate::dependent");
    assert_eq!(preview.outbound[0].edge_type, "Influences");
    assert_eq!(preview.evidence_completeness, "partial");
    assert!(!preview.executable);
    assert!(preview
        .blockers
        .iter()
        .any(|blocker| blocker.contains("inbound graph relationship")));
    assert!(preview
        .blockers
        .iter()
        .any(|blocker| blocker.contains("reference completeness")));
    assert_eq!(
        preview
            .verification
            .iter()
            .map(|step| step.order)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert!(require_executable(&preview).is_err());
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/target.rs")).unwrap(),
        before
    );
}

#[test]
fn test_deletion_preview_revision_is_stable_for_the_same_graph() {
    let (_project, ide, graph) = deletion_fixture();

    let first = preview_deletion(&graph, &ide, "crate::target").unwrap();
    let second = preview_deletion(&graph, &ide, "crate::target").unwrap();

    assert_ne!(first.action_id, second.action_id);
    assert_eq!(first.target.graph_revision, second.target.graph_revision);
    assert_eq!(first.target.content_hash, second.target.content_hash);
}

#[test]
fn test_deletion_preview_rejects_unknown_and_pathless_nodes() {
    let (_project, ide, mut graph) = deletion_fixture();
    graph.nodes.push(selfware::evolve::Node {
        id: "concept".to_string(),
        layer: selfware::evolve::NodeLayer::Concept,
        path: None,
        tokens: 0,
        lines: 0,
        files: 0,
        coverage: None,
        dead_code_ratio: None,
        warning_count: None,
        complexity: None,
        inline_test_ranges: 0,
        inline_test_lines: 0,
        inline_test_tokens: 0,
        classification: "concept".to_string(),
    });

    assert!(preview_deletion(&graph, &ide, "missing").is_err());
    assert!(preview_deletion(&graph, &ide, "concept").is_err());
}
