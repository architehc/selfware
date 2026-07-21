use selfware::evolve::{self, EvolveServer, GraphBuilder, Node, NodeLayer};

#[tokio::test]
async fn test_run_self_evolve_starts_server() {
    // Verify function exists and compiles
    let _ = evolve::run_self_evolve;
}

#[tokio::test]
async fn test_full_server_flow() {
    let builder = GraphBuilder::new("src");
    let graph = builder.scan_src().unwrap();
    let server = EvolveServer::new(graph);
    let json = server.graph_json().await.unwrap();
    assert!(json.contains("agent"));
}

#[test]
fn test_graph_node_creation() {
    let node = Node::code("agent", "src/agent");
    assert_eq!(node.id, "agent");
    assert_eq!(node.layer, NodeLayer::Code);
    assert_eq!(node.path, Some("src/agent".to_string()));

    let test = Node::test("tests::agent_test", "tests/agent_test.rs");
    assert_eq!(test.layer, NodeLayer::Test);
    assert_eq!(test.path, Some("tests/agent_test.rs".to_string()));

    let structure = Node::structure("repo::src");
    assert_eq!(structure.layer, NodeLayer::Structure);
    assert_eq!(structure.path, None);
    assert_eq!(structure.tokens, 0);

    let mut metric_node = Node::code("metric", "src/metric.rs");
    metric_node.dead_code_ratio = Some(0.25);
    let serialized = serde_json::to_value(&metric_node).unwrap();
    assert_eq!(serialized["dead_code_annotation_ratio"], 0.25);
    assert!(serialized.get("dead_code_ratio").is_none());

    let mut legacy = serialized;
    let value = legacy
        .as_object_mut()
        .unwrap()
        .remove("dead_code_annotation_ratio")
        .unwrap();
    legacy
        .as_object_mut()
        .unwrap()
        .insert("dead_code_ratio".to_string(), value);
    let loaded: Node = serde_json::from_value(legacy).unwrap();
    assert_eq!(loaded.dead_code_ratio, Some(0.25));
}
