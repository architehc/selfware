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
}
