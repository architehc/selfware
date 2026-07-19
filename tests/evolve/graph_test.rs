use selfware::evolve::graph::GraphBuilder;

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
    let ids: std::collections::HashSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    // File modules must be normalized to bare names (no `.rs` suffix).
    assert!(ids.iter().all(|id| !id.ends_with(".rs")));
    for edge in &graph.edges {
        assert!(ids.contains(edge.from.as_str()), "dangling from: {}", edge.from);
        assert!(ids.contains(edge.to.as_str()), "dangling to: {}", edge.to);
    }
}
